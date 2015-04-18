use regex::Regex;
use std::io;
use std::io::prelude::*;
use std::str::from_utf8;
use std::sync::{Arc, Mutex, Future};
use std::sync::mpsc;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use super::channels;

/// What should we do next?
pub enum Action {
    /// Don't run any more handlers on this line.
    Skip,
    /// Try the rest of the handlers on this line.
    Continue,
    /// Quit the event loop.
    Stop,
}

pub enum HandlerAction {
    Add(Handler),
    Swap(Handler),
    Keep,
    Remove,
}

/// Optionally respond with a message, then take the given next `Action`.
pub struct Response(pub Option<String>, pub HandlerAction, pub Action);

impl Response {

    pub fn respond(s: String) -> Response {
        Response(Some(s), HandlerAction::Keep, Action::Skip)
    }

    pub fn nothing() -> Response {
        Response(None, HandlerAction::Keep, Action::Continue)
    }
}

/// A `Handler` examines a line of input read from a stream, and
/// produces an optional `Response` to send, an additional `Handler`
/// to install (perhaps to wait for a response), and a next `Action`
/// to take after processing the line.
pub type Handler = Box<FnMut(&str) -> Response + Send>;

/// `EventStream` wraps a stream and will loop, reading lines and
/// processing them with `Handler`s, which are allowed to write back
/// to the stream and install additional `Handler`s.
///
/// It implements `Write` so you can still write manually, and you can
/// install additional `Handler`s when you want.
pub struct EventStream {
    writer: Sender<String>,
    handlers: Arc<Mutex<Vec<Handler>>>,
}

impl EventStream {

    /// Creates a new event stream with initial handlers.  You should
    /// pass the `Read` and `Write` parts separately, for example using
    /// `TcpStream::try_clone`.
    pub fn new<R: Read + Send + 'static, W: Write + Send + 'static>(inner_reader: R, inner_writer: W, init_handlers: Vec<Handler>) -> io::Result<(EventStream, thread::JoinHandle<()>)> {
        let reader = channels::reader(inner_reader);
        let writer = channels::writer(inner_writer);
        let handlers = Arc::new(Mutex::new(init_handlers));
        let thread_writer = writer.clone();
        let thread_handlers = handlers.clone();
        let join_handle = thread::spawn(move || {
            event_loop(reader, thread_writer, thread_handlers)
                .err().and_then(|e| -> Option<()> {
                    if let channels::ChanError::SendError(mpsc::SendError(l)) = e {
                        error!("Send of \"{}\" failed, channel is disconnected.", l);
                    } else {
                        error!("Receive on channel failed, channel is disconnected.");
                    }
                    None
                });
        });
        let stream = EventStream{
            writer: writer,
            handlers: handlers,
        };
        Ok((stream, join_handle))
    }

    pub fn add_handler(&mut self, handler: Handler) {
        self.handlers.lock().unwrap().push(handler);
    }

    pub fn await_lines(&mut self, expectations: Vec<Regex>) -> Vec<Future<String>> {
        expectations.into_iter().map(|expectation| {
            let (tx, rx) = channel();
            self.add_handler(box move |line| {
                if expectation.is_match(line) {
                    tx.send(line.to_string())
                        .ok().expect("Couldn't send awaited line.");;
                    Response(None, HandlerAction::Remove, Action::Skip)
                } else {
                    Response::nothing()
                }
            });
            Future::from_receiver(rx)
        }).collect()
    }

}

impl Write for EventStream {

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let line = from_utf8(buf).unwrap().to_string();
        self.writer.send(line)
            .and(Ok(buf.len()))
            .or(Err(io::Error::new(io::ErrorKind::NotConnected, "Send failed, channel is disconnected.")))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

}

fn process_one_event(line: &str, writer: &Sender<String>, handlers: &mut Vec<Handler>) -> Result<Action, mpsc::SendError<String>> {
    let mut i: usize = 0;
    while i < handlers.len() {
        let Response(msg, handler_action, action) = {
            let h = &mut handlers[i];
            h(line)
        };
        // Send a message, if any.
        if let Some(m) = msg {
            try!(writer.send(m));
        }
        // Increment i if we didn't remove the handler here.
        match handler_action {
            HandlerAction::Remove => (),
            _ => { i = i + 1; },
        };
        // Modify handlers, if needed.
        match handler_action {
            HandlerAction::Add(h) => {
                handlers.push(h);
            },
            HandlerAction::Swap(h) => {
                handlers[i] = h;
            },
            HandlerAction::Remove => {
                handlers.swap_remove(i);
            },
            HandlerAction::Keep => (),
        };
        // Exit early, if requested.
        match action {
            Action::Continue => (),
            _ => {
                return Ok(action);
            },
        };
    }
    Ok(Action::Continue)
}

fn event_loop(reader: Receiver<String>, writer: Sender<String>, handlers: Arc<Mutex<Vec<Handler>>>) -> Result<(), channels::ChanError<String>> {
    loop {
        let line = try!(reader.recv().map_err(channels::recv_to_chan_error));
        match try!(process_one_event(&line, &writer, &mut *handlers.lock().unwrap()).map_err(channels::send_to_chan_error)) {
            Action::Stop => {
                info!("Exiting event loop...");
                break;
            },
            _ => (),
        };
    }
    Ok(())
}
