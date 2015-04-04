use regex::Regex;
use std::io;
use std::io::prelude::*;
use super::event_stream::{Action, HandlerAction, Response, EventStream};

static MODE_WALLOPS: u16 = 4;
static MODE_INVISIBLE: u16 = 8;

fn mode_for(invisible: bool, wallops: bool) -> u16 {
    (if invisible { MODE_WALLOPS } else { 0 }) + (if wallops { MODE_INVISIBLE } else { 0 })
}

pub fn pong_handler(line: &str) -> Response {
    //debug!("pong_handler considering message \"{}\"", s);
    if line.starts_with("PING") {
        let resp = format!("PONG {}\r\n", line.split(' ').skip(1).next().or(Some("")).unwrap());
        //debug!("pong_handler responding \"{}\"", resp);
        Response::respond(resp.to_string())
    } else {
        //debug!("pong_handler returning Continue");
        Response::nothing()
    }
}

pub fn timeout_handler(line: &str) -> Response {
    if line.starts_with("ERROR :Closing Link:") {
        Response(None, HandlerAction::Keep, Action::Stop)
    } else {
        Response::nothing()
    }
}

pub fn login(stream: &mut EventStream, nick: &str,
             user: &str, realname: &str,
             invisible: bool, wallops: bool) -> io::Result<String> {
    let re = Regex::new(r"^([^\s]+)\s+001").unwrap();
    let server_responses = stream.await_lines(vec![re.clone()]);
    let login_responses = stream.await_lines(vec![Regex::new(&format!(r"MODE {} :\+[iswx]+$", nick))
                                                  .ok().expect("Regex compilation failure: bad username?")]);

    try!(write!(stream, "NICK {}\r\n", nick));
    try!(write!(stream, "USER {} {} unused {}\r\n", user, mode_for(invisible, wallops), realname));

    login_responses.into_iter().map(|mut f| f.get()).next().expect("Didn't get login response.");

    Ok(server_responses.into_iter().map(|mut f| {
        re.captures(&f.get())
            .expect("Regex match was bad")
            .at(1).expect("Regex didn't capture").to_string()
    }).next().expect("Didn't get server response"))
}

pub fn join(stream: &mut EventStream, server: &str, channels: &[&str]) -> io::Result<()> {
    let regexes = channels.iter().map(|chan| Regex::new(&format!(r"JOIN\s+:?{}", chan)).unwrap()).collect();
    let responses = stream.await_lines(regexes);

    for chan in channels.iter() {
        info!("Joining channel {}...", chan);
        try!(write!(stream, "{} JOIN {}\r\n", server, chan));
    }
    let re = Regex::new(r"JOIN\s+:?([^\s]+)").unwrap();
    responses.into_iter().map(|f| {
        info!("Joined channel {}!", re.captures(&f.into_inner()).unwrap().at(1).unwrap());
    }).count();
    Ok(())
}

pub struct UserInfo<'a> {
    pub nick: &'a str,
    pub user: Option<&'a str>,
    pub host: Option<&'a str>,
}

impl<'a> UserInfo<'a> {

    pub fn parse(s: &'a str) -> Option<UserInfo<'a>> {
        Regex::new(r"^([][\\`_^{|}a-zA-Z][][\\`_^{|}a-zA-Z0-9-]*)((!([^ @]+))?@(.*))?$").unwrap()
            .captures(s).map(|c| {
                UserInfo{
                    nick: c.at(1).expect("Bad match group"),
                    user: c.at(4),
                    host: c.at(5),
                }
            })
    }

}

pub enum Source<'a> {
    Server(&'a str),
    User(UserInfo<'a>),
}

impl<'a> Source<'a> {

    pub fn parse(s: &'a str) -> Option<Source<'a>> {
        UserInfo::parse(s).map(|u| Source::User(u))
            .or(Some(Source::Server(s)))
    }

}

pub enum Dest<'a> {
    Nick(&'a str),
    Chan(&'a str),
}

impl<'a> Dest<'a> {

    pub fn parse(s: &'a str) -> Option<Dest<'a>> {
        let chan_re = Regex::new(r"^[#+&!][^,:]+$").unwrap();
        if chan_re.is_match(s) {
            Some(Dest::Chan(s))
        } else {
            Some(Dest::Nick(s))
        }
    }

    pub fn format(&self) -> String {
        match self {
            &Dest::Nick(n) => n.to_string(),
            &Dest::Chan(c) => c.to_string(),
        }
    }

}

pub struct Privmsg<'a> {
    pub src: Option<Source<'a>>,
    pub dst: Dest<'a>,
    pub msg: &'a str,
}

impl<'a> Privmsg<'a> {

    pub fn parse(line: &'a str) -> Option<Privmsg<'a>> {
        Regex::new(r"^:([^\s]+)\s+PRIVMSG\s+([^\s]+)\s+:?(.*)$").unwrap()
            .captures(line).map(
                |c| Source::parse(c.at(1).expect("Bad match group")).map(
                    |src| Dest::parse(c.at(2).expect("Bad match group")).map(
                        |dst| Privmsg{
                            src: Some(src),
                            dst: dst,
                            msg: c.at(3).expect("Bad match group"),
                        })
                        .unwrap())
                    .unwrap())
    }

    pub fn new(dst: Dest<'a>, msg: &'a str) -> Privmsg<'a> {
        Privmsg{
            src: None,
            dst: dst,
            msg: msg,
        }
    }

    pub fn format(&self) -> String {
        format!("PRIVMSG {} :{}", self.dst.format(), self.msg)
    }

    pub fn reply_target(&'a self, nick: &str) -> Option<Dest<'a>> {
        match self.dst {
            Dest::Nick(ref n) if *n == nick => {
                // This is a PM, we should respond directly to the user that sent it.
                if let Some(Source::User(ref user_info)) = self.src {
                    Some(Dest::Nick(user_info.nick))
                } else {
                    None
                }
            },
            Dest::Chan(ref chan) => Some(Dest::Chan(chan)),
            _ => {
                //error!("How'd we get this message? \"{}\"", line);
                None
            },
        }
    }

    pub fn targeted_msg(self, nick: &str) -> Option<Privmsg<'a>> {
        if let Dest::Nick(n) = self.dst {
            assert_eq!(n, nick);
            Some(self)
        } else if let Some(c) = Regex::new(&format!(r"^{}[:,]?\s+(.*)\s*$", nick)).ok().expect("bad regex").captures(self.msg) {
            Some(Privmsg{
                src: self.src,
                dst: self.dst,
                msg: c.at(1).expect("Bad regex group"),
            })
        } else {
            None
        }
    }

}
