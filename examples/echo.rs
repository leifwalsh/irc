#![feature(box_syntax,plugin)]
#![plugin(regex_macros)]

use irc::event_stream::{Action, HandlerAction, Response};
use irc::protocol;

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

    let echo_handler = box move |line: &str| {
        if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(nick)) {
            if regex!(r"^[Hh]i$").is_match(pm.msg) {
                if let Some(reply_to) = pm.reply_target(nick) {
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
        &addr, nick, &channels, "rustbot", "rust irc robot")
        .ok().expect(&format!("Error connecting to {:?}.", addr));

    client.add_handler(echo_handler);
    join_handle.join().unwrap_or_else(|_| {
        error!("Unknown error!");
    });
}
