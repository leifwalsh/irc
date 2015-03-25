#![feature(box_syntax)]

use irc::event_stream::{Action, HandlerAction, Response};
use irc::protocol;
use regex::Regex;

extern crate env_logger;
extern crate irc;
extern crate regex;

#[macro_use]
extern crate log;

fn main() {
    env_logger::init().unwrap();

    let addr = ("irc.freenode.net", 6667);
    let nick = "rustbot_test";
    let channels = ["#rustbot_test"];

    // let addr = ("irc.freenode.net", 6667);
    // let nick = "hiphopabotamus";
    // let channels = ["#raptracks"];

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
        &addr, nick, &channels, "rustbot", "rust irc robot")
        .ok().expect(&format!("Error connecting to {:?}.", addr));

    client.add_handler(echo_handler);
    join_handle.join().unwrap_or_else(|_| {
        error!("Unknown error!");
    });
}
