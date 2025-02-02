use std::io;
use std::io::prelude::*;
use std::net::TcpStream;
use std::thread::sleep;
use std::time::Duration;
use std::net::UdpSocket;
use network_rust::udpnet;

fn main() {
    tcp();
    
}

fn tcp() -> io::Result<()> {
    let ip = "10.24.150.137:34933";
    let mut read_buffer = [0; 1024];

    let mut stream = TcpStream::connect(ip)?;

    stream.read(&mut read_buffer)?;
    println!("{}", String::from_utf8_lossy(&read_buffer));

    let mut to_send = String::new();


    loop {
        to_send.clear();
        read_buffer = [0; 1024];

        io::stdin()
            .read_line(&mut to_send)
            .expect("An error occured");

        let mut to_send = String::from(to_send.trim());

        to_send.push('\0');

        stream.write(&to_send.as_bytes()).expect("An error occured");
        sleep(Duration::new(0, 500000)); // Sleep for 0.5 seconds
        stream.read(&mut read_buffer).expect("An error occured on read");
        println!("{}", String::from_utf8_lossy(&read_buffer));
    };
}

fn udp() -> io::Result<()> {
    // Create a UDP socket
    let socket = UdpSocket::bind("0.0.0.0:30000")?;

    loop {
        // Create a buffer to receive data
        let mut buf = [0; 1024];

        // Receive data from the socket
        let (len, src) = socket.recv_from(&mut buf)?;

        // Print the received data and source address
        println!("Received {} bytes from {:?}", len, src);
        println!("Data: {:?}", String::from_utf8_lossy(&buf[..len]));
    }
}