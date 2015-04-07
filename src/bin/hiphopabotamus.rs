#![feature(box_syntax)]
#![feature(plugin)]
#![plugin(regex_macros)]

use irc::event_stream::{Action, HandlerAction, Response};
use irc::protocol;
use regex::Regex;
use rand::{thread_rng, Rng};
use std::ascii::AsciiExt;
use std::collections::{hash_map, HashMap};
use std::env;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

extern crate env_logger;
extern crate getopts;
extern crate irc;
extern crate rand;
extern crate regex;

#[macro_use]
extern crate log;

fn main() {
    env_logger::init().unwrap();

    let mut opts = getopts::Options::new();
    opts.reqopt("", "host", "irc server hostname", "HOSTNAME");
    opts.optopt("", "port", "irc server port", "PORT");
    opts.reqopt("n", "nick", "nickname", "NICK");
    opts.optmulti("c", "chan", "channels to join", "CHAN");

    let args: Vec<String> = env::args().collect();
    let matches = opts.parse(&args[1..]).unwrap();

    let host = matches.opt_str("host").expect("must provide --host");
    let mut port = 6667;
    if matches.opt_present("port") {
        port = u16::from_str(&matches.opt_str("port").expect("must provide argument to --port")).unwrap();
    }

    let addr = (host.as_ref(), port);
    let nick = matches.opt_str("nick").expect("must provide --nick");
    let channels = matches.opt_strs("chan");

    let knowledge_mutex: Arc<Mutex<HashMap<String, Vec<String>>>> = Arc::new(Mutex::new(HashMap::new()));

    let choice_nick = nick.clone();
    let choice_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(&choice_nick)) {
            let choices: Vec<_> = regex!(r"\s+or\s+")
                .split(pm.msg)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if choices.len() > 1 {
                let mut rng = thread_rng();
                if let Some(reply_to) = pm.reply_target(&choice_nick) {
                    return Response::respond(protocol::Privmsg::new(reply_to, rng.choose(&choices).unwrap()).format());
                }
            }
        }
        Response::nothing()
    };

    let knowledge_mutex_1 = knowledge_mutex.clone();
    let learning_nick = nick.clone();
    let learning_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(&learning_nick)) {
            let assignment: Vec<_> = regex!(r"\s+is\s+")
                .splitn(pm.msg, 2)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if assignment.len() == 2 {
                let mut knowledge = knowledge_mutex_1.lock().unwrap();
                match knowledge.entry(assignment[0].to_string()) {
                    hash_map::Entry::Occupied(mut e) => {
                        e.get_mut().push(assignment[1].to_string());
                    },
                    hash_map::Entry::Vacant(e) => {
                        e.insert(vec![assignment[1].to_string()]);
                    }
                }
            }
        }
        Response::nothing()
    };

    let knowledge_mutex_2 = knowledge_mutex.clone();
    let info_nick = nick.clone();
    let info_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line) {
            if let Some(reply_to) = pm.reply_target(&info_nick) {
                let knowledge = knowledge_mutex_2.lock().unwrap();
                if let Some(choices) = knowledge.get(pm.msg) {
                    let mut rng = thread_rng();
                    return Response::respond(protocol::Privmsg::new(reply_to, rng.choose(choices).unwrap()).format());
                }
            }
        }
        Response::nothing()
    };

    let join_nick = nick.clone();
    let join_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(&join_nick)) {
            if let Some(c) = regex!(r"^(join|part) (#[^\s]+)").captures(pm.msg) {
                let action = c.at(1).expect("Bad match group");
                let chan = c.at(2).expect("Bad match group");
                let success_msg = format!("{} channel {}!", if action == "join" { "Joined" } else { "Left" }, chan);
                if let Ok(joined_regex) = Regex::new(&format!(r"{}\s+:?{}", action.to_ascii_uppercase(), chan)) {
                    let joined_handler = box move |joined_line: &str| {
                        if joined_regex.is_match(joined_line) {
                            info!("{}", success_msg);
                            return Response(None, HandlerAction::Remove, Action::Skip);
                        }
                        Response::nothing()
                    };
                    info!("{} channel {}...", if action == "join" { "Joining" } else { "Leaving" }, chan);
                    return Response(Some(format!("{} {}", action.to_ascii_uppercase(), chan)), HandlerAction::Add(joined_handler), Action::Skip);
                }
            }
        }
        Response::nothing()
    };

    let echo_nick = nick.clone();
    let echo_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(&echo_nick)) {
            if regex!(r"^[Hh]i$").is_match(pm.msg) {
                if let Some(reply_to) = pm.reply_target(&echo_nick) {
                    if let Some(protocol::Source::User(ref user_info)) = pm.src {
                        return Response::respond(protocol::Privmsg::new(reply_to, &format!("Hi, {}!", user_info.nick)).format());
                    }
                }
            } else if regex!(r"^go away$").is_match(pm.msg) {
                return Response(None, HandlerAction::Keep, Action::Stop);
            }
        }
        Response::nothing()
    };

    let (mut client, join_handle) = irc::Client::connect(
        &addr, &nick, &channels.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(), "leifw_rustbot", "leifw's rust robot")
        .ok().expect(&format!("Error connecting to {:?}.", addr));

    client.add_handler(choice_handler);
    client.add_handler(learning_handler);
    client.add_handler(info_handler);
    client.add_handler(join_handler);
    client.add_handler(echo_handler);
    join_handle.join().unwrap_or_else(|_| { error!("Unknown error!"); });
}
