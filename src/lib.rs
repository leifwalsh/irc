#![feature(box_syntax,plugin,std_misc,unboxed_closures)]
#![plugin(regex_macros)]

//! Provides some basic functionality for connecting to IRC servers
//! and performing robotic tasks.
//!
//! # Example:
//! ```{.ignore .rust}
//! use irc::event_stream::Response;
//! use irc::protocol;
//!
//! let addr = ("irc.freenode.net", 6667);
//! let nick = "rustbot_test";
//! let channels = ["#rustbot_test"];
//!
//! let echo_handler = move |line: &str| {
//!     if let Some(pm) = protocol::Privmsg::parse(line).and_then(|pm| pm.targeted_msg(nick)) {
//!         if pm.msg == "foo" {
//!             if let Some(reply_to) = pm.reply_target(nick) {
//!                 return Response::respond(protocol::Privmsg::new(reply_to, "bar").format());
//!             }
//!         }
//!     }
//!     Response::nothing()
//! };
//!
//! let (mut client, join_handle) = irc::Client::connect(
//!     &addr, nick, &channels, "rustbot", "rust irc robot")
//!     .ok().expect(&format!("Error connecting to {:?}.", addr));
//!
//! client.add_handler(Box::new(echo_handler));
//! join_handle.join().ok().unwrap();
//! ```

use event_stream::{Handler, EventStream, Response};
use std::fmt;
use std::io;
use std::net;
use std::thread;

mod channels;
pub mod event_stream;
pub mod protocol;

#[macro_use]
extern crate log;
extern crate regex;

/// The top-level IRC client.
pub struct Client {
    stream: EventStream,
    server: String,
}

impl Client {

    /// Connects to an IRC server.
    ///
    /// Returns a `Client` object to which you can add handlers, and a
    /// `thread::JoinHandle` which will join when the thread handling
    /// IRC events finishes.
    ///
    /// # Example:
    /// ```{.ignore .rust}
    /// let host = "irc.freenode.net";
    /// let port = 6667;
    /// let nick = "rustbot_test";
    /// let channels = ["#rustbot_test"];
    /// let (mut client, join_handle) = irc::Client::connect(
    ///     &(host, port), nick, &channels, "username", "realname").ok().unwrap();
    /// ```
    pub fn connect<A: net::ToSocketAddrs + fmt::Debug>(addr: &A,
                                                       nick: &str, channels: &[&str],
                                                       user: &str, realname: &str) -> io::Result<(Client, thread::JoinHandle<()>)> {
        Client::connect_mode(addr, nick, channels, user, realname, false, true)
    }

    /// Connects to an IRC server with additional options `invisible` and `wallops`.
    ///
    /// See `Client::connect` and the IRC protocol RFC for details.
    pub fn connect_mode<A: net::ToSocketAddrs + fmt::Debug>(addr: &A,
                                                            nick: &str, channels: &[&str],
                                                            user: &str, realname: &str,
                                                            invisible: bool, wallops: bool) -> io::Result<(Client, thread::JoinHandle<()>)> {
        let default_handlers: Vec<Handler> = vec![
            box protocol::pong_handler,
            box protocol::timeout_handler,
            ];

        debug!("Connecting to {:?}...", addr);
        let conn = try!(net::TcpStream::connect(addr));
        let conn_copy = try!(conn.try_clone());

        let (mut stream, join_handle) = try!(EventStream::new(conn, conn_copy, default_handlers));
        info!("Connected to {:?}!", addr);

        let server = try!(protocol::login(&mut stream, nick, user, realname, invisible, wallops));
        info!("Logged in at \"{}\"!", server);

        try!(protocol::join(&mut stream, &server, channels));

        let client = Client{
            stream: stream,
            server: server,
        };
        Ok((client, join_handle))
    }

    /// Adds a new handler to the event loop.
    ///
    /// # Example:
    /// ```{.ignore .rust}
    /// use irc::event_stream::Response;
    ///
    /// let (mut client, _) = irc::Client::connect(
    ///     &("irc.freenode.net", 6667), "rustbot", &[], "rustbot", "rustbot").ok().unwrap();
    /// client.add_handler(Box::new(move |line: &str| {
    ///     // A literal echo server like this would really confuse a real IRC server...
    ///     Response::respond(line.to_string())
    /// }));
    /// ```
    pub fn add_handler(&mut self, handler: Handler) {
        let server = self.server.clone();
        let mut handler_mut = handler;
        self.stream.add_handler(box move |line| {
            let Response(msg, ha, a) = handler_mut(line);
            if let Some(s) = msg {
                Response(Some(format!("{} {}\r\n", &server, s)), ha, a)
            } else {
                Response(msg, ha, a)
            }
        });
    }

}

impl Drop for Client {

    /// Sends a QUIT message before dropping.
    fn drop(&mut self) {
        use std::io::Write;

        info!("Quitting from server...");
        write!(&mut self.stream, "{} QUIT: adios\r\n", &self.server)
            .err().and_then(|e| -> Option<()> {
                error!("Error quitting: {:?}", e);
                None
            });
    }

}
