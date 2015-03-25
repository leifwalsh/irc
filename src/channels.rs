use std::io;
use std::io::prelude::*;
use std::sync::mpsc;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

pub enum ChanError<T> {
    RecvError(mpsc::RecvError),
    SendError(mpsc::SendError<T>),
}

pub fn send_to_chan_error<T>(e: mpsc::SendError<T>) -> ChanError<T> {
    ChanError::SendError(e)
}

pub fn recv_to_chan_error<T>(e: mpsc::RecvError) -> ChanError<T> {
    ChanError::RecvError(e)
}

fn reader_loop<R: Read>(r: R, tx: Sender<String>) -> io::Result<()> {
    let mut reader = io::BufReader::new(r);
    loop {
        let mut line = String::new();
        try!(reader.read_line(&mut line));
        line = line.trim().to_string();
        debug!("Read \"{}\".", line);
        match tx.send(line) {
            Ok(_) => (),
            Err(mpsc::SendError(l)) => {
                debug!("Send of \"{}\" failed, channel is disconnected.", l);
                return Ok(());
            },
        };
    }
}

/// Reads lines from the provided `Read` and sends them into a
/// channel.  Returns the `Receiver` of that channel.
pub fn reader<R: Read + Send + 'static>(r: R) -> Receiver<String> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        match reader_loop(r, tx) {
            Err(e) => {
                error!("Fatal I/O error \"{:?}\".", e);
            },
            Ok(_) => (),
        }
    });
    rx
}

fn writer_loop<W: Write>(w: W, rx: Receiver<String>) -> io::Result<()> {
    let mut writer = io::LineWriter::new(w);
    let mut buf = String::new();
    for line in rx.iter() {
        buf.push_str(line.as_slice());
        if buf.ends_with('\n') {
            debug!("Sending \"{}\"...", buf.trim());
            buf = String::new();
        }
        try!(writer.write(line.as_bytes()));
    }
    Ok(())
}

/// Creates a channel that will write lines it receives to the
/// provided `Write`.  Returns the `Sender` half of the channel.
pub fn writer<W: Write + Send + 'static>(w: W) -> Sender<String> {
    let (tx, rx) = channel();
    thread::spawn(move || {
        match writer_loop(w, rx) {
            Err(e) => {
                error!("Fatal I/O error \"{:?}\".", e);
            },
            Ok(_) => (),
        }
    });
    tx
}
