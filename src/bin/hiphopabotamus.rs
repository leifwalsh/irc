#![feature(box_syntax)]

use irc::event_stream::{Action, HandlerAction, Response};
use irc::protocol;
use regex::Regex;
use rand::{thread_rng, Rng};
use std::collections::{hash_map, HashMap};
use std::sync::{Arc, Mutex};

extern crate env_logger;
extern crate irc;
extern crate rand;
extern crate regex;

#[macro_use]
extern crate log;

fn main() {
    env_logger::init().unwrap();

    // let addr = ("irc.foonetic.net", 6667);
    // let nick = "hiphopabotamus";
    // let channels = ["#botwerks"];

    let addr = ("irc.freenode.net", 6667);
    let nick = "hiphopabotamus";
    let channels = ["#raptracks"];

    let knowledge_mutex: Arc<Mutex<HashMap<String, Vec<String>>>> = Arc::new(Mutex::new(HashMap::new()));

    let choice_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(nick)) {
            let choices: Vec<_> = Regex::new(r"\s+or\s+").unwrap()
                .split(pm.msg)
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if choices.len() > 1 {
                let mut rng = thread_rng();
                if let Some(reply_to) = pm.reply_target(nick) {
                    return Response::respond(protocol::Privmsg::new(reply_to, rng.choose(&choices).unwrap()).format());
                }
            }
        }
        Response::nothing()
    };

    let knowledge_mutex_1 = knowledge_mutex.clone();
    let learning_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(nick)) {
            let assignment: Vec<_> = Regex::new(r"\s+is\s+").unwrap()
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
    let info_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line) {
            if let Some(reply_to) = pm.reply_target(nick) {
                let knowledge = knowledge_mutex_2.lock().unwrap();
                if let Some(choices) = knowledge.get(pm.msg) {
                    let mut rng = thread_rng();
                    return Response::respond(protocol::Privmsg::new(reply_to, rng.choose(choices).unwrap()).format());
                }
            }
        }
        Response::nothing()
    };

    let echo_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(nick)) {
            if Regex::new(r"^[Hh]i$").ok().expect("bad regex").is_match(pm.msg) {
                if let Some(reply_to) = pm.reply_target(nick) {
                    if let Some(protocol::Source::User(ref user_info)) = pm.src {
                        return Response::respond(protocol::Privmsg::new(reply_to, &format!("Hi, {}!", user_info.nick)).format());
                    }
                }
            } else if Regex::new(r"^go away$").unwrap().is_match(pm.msg) {
                return Response(None, HandlerAction::Keep, Action::Stop);
            }
        }
        Response::nothing()
    };

    let (mut client, join_handle) = irc::Client::connect(
        &addr, nick, &channels, "leifw_rustbot", "leifw's rust robot")
        .ok().expect(&format!("Error connecting to {:?}.", addr));

    client.add_handler(choice_handler);
    client.add_handler(learning_handler);
    client.add_handler(info_handler);
    client.add_handler(echo_handler);
    join_handle.join().unwrap_or_else(|_| {
        error!("Unknown error!");
    });
}
