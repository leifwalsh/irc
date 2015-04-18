#![feature(box_syntax)]
#![feature(plugin)]
#![plugin(postgres_macros,regex_macros)]

use irc::event_stream::{Action, HandlerAction, Response};
use irc::protocol;
use postgres::{Connection, SslMode};
use regex::Regex;
use rand::{thread_rng, Rng};
use std::ascii::AsciiExt;
use std::env;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

extern crate env_logger;
extern crate getopts;
extern crate irc;
extern crate postgres;
extern crate rand;
extern crate regex;

#[macro_use]
extern crate log;

fn init_pg_tables() -> Connection {
    let pg = Connection::connect("postgres://leif@localhost", &SslMode::None).unwrap();
    pg.execute("CREATE TABLE IF NOT EXISTS knowledge (
                  key VARCHAR,
                  val VARCHAR,
                  PRIMARY KEY(key, val)
                )", &[]).unwrap();
    pg.execute("CREATE TABLE IF NOT EXISTS karma (
                  nick VARCHAR PRIMARY KEY,
                  karma INTEGER NOT NULL DEFAULT 0
                )", &[]).unwrap();
    pg
}

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

    let pg = Arc::new(Mutex::new(init_pg_tables()));

    // {
    //     // TODO: Seed our knowledge
    //     let mut knowledge = knowledge_mutex.lock().unwrap();
    //     knowledge.insert(format!("{}: info", nick), vec!["I'm a robot running https://github.com/leifwalsh/irc, feel free to contribute!".to_string()]);
    //     knowledge.insert(format!("{}, info", nick), vec!["I'm a robot running https://github.com/leifwalsh/irc, feel free to contribute!".to_string()]);
    // }

    let learning_pg = pg.clone();
    let learning_nick = nick.clone();
    let learning_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(&learning_nick)) {
            let assignment: Vec<_> = regex!(r"\s+is\s+")
                .splitn(pm.msg, 2)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if assignment.len() == 2 {
                let conn = learning_pg.lock().unwrap();
                conn.execute(sql!("INSERT INTO knowledge (key, val) VALUES ($1, $2)"),
                             &[&assignment[0], &assignment[1]]).unwrap();
            }
        }
        Response::nothing()
    };

    let info_pg = pg.clone();
    let info_nick = nick.clone();
    let info_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line) {
            if let Some(reply_to) = pm.reply_target(&info_nick) {
                let mut rng = thread_rng();
                let conn = info_pg.lock().unwrap();
                let info_stmt = conn.prepare(sql!("SELECT val FROM knowledge WHERE key = $1")).unwrap();
                if let Some(choice) = rand::sample(&mut rng, info_stmt.query(&[&pm.msg]).unwrap().into_iter().map(|r| { r.get::<_, String>(0) }), 1).get(0) {
                    if let Some(c) = regex!(r"^<action>\s+(.*)$").captures(choice) {
                        return Response::respond(protocol::Privmsg::new(reply_to, &protocol::ctcp_action(c.at(1).expect("Bad regex match"))).format());
                    } else {
                        return Response::respond(protocol::Privmsg::new(reply_to, choice).format());
                    }
                }
            }
        }
        Response::nothing()
    };

    let karma_pg = pg.clone();
    let karma_nick = nick.clone();
    let karma_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line) {
            if let Some(c) = regex!(r"^([^-+\s]+)(\+\+|--)$").captures(pm.msg) {
                let n = c.at(1).expect("Bad match group");
                let inc = c.at(2).expect("Bad match group") == "++";
                let conn = karma_pg.lock().unwrap();
                let stmt = conn.prepare(sql!("SELECT count(nick) FROM karma WHERE nick = $1")).unwrap();
                let count: i64 = stmt.query(&[&n]).unwrap().into_iter().next().unwrap().get(0);
                let change = if inc { 1 } else { -1 };
                if count == 0 {
                    conn.execute(sql!("INSERT INTO karma (nick, karma) VALUES ($1, $2)"), &[&n, &change]).unwrap();
                } else {
                    conn.execute(sql!("UPDATE karma SET karma = karma + $2 WHERE nick = $1"), &[&n, &change]).unwrap();
                }
            } else if let Some(c) = regex!(r"^karma\s+([^\s]+)$").captures(pm.msg) {
                if let Some(reply_to) = pm.reply_target(&karma_nick) {
                    let n = c.at(1).expect("Bad match group");
                    let conn = karma_pg.lock().unwrap();
                    let stmt = conn.prepare(sql!("SELECT karma FROM karma WHERE nick = $1")).unwrap();
                    let k: i32 = stmt.query(&[&n]).unwrap()
                        .into_iter().next().and_then(|r| r.get(0)).or(Some(0)).unwrap();
                    return Response::respond(protocol::Privmsg::new(reply_to, &format!("{}: {}", n, k)).format())
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
    client.add_handler(karma_handler);
    client.add_handler(info_handler);
    client.add_handler(join_handler);
    client.add_handler(echo_handler);
    join_handle.join().unwrap_or_else(|_| { error!("Unknown error!"); });
}
