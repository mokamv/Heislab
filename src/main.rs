use std::io::prelude::*;
use std::net::TcpStream;

fn main() -> std::io::Result<()> {
    let ip = "10.24.150.137:34933";
    let message = String::from("Hi!");
    let mut buffer = [0; 128];

    let mut stream = TcpStream::connect(ip)?;

    stream.write(&message.as_bytes())?;
    stream.read(&mut buffer)?;

    println!("{}", String::from_utf8_lossy(&buffer));

    Ok(())
} // the stream is closed here