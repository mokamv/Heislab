## Network design
> Master/Slave  (Controller + Backup + Elevator)

We should use a watchdog timer
Slave becoming master? Maybe but only wth enough time.


> UDP/TCP

UDP as an initialization layer + authentication?
1. Master broadcast with UDP and start backup as a master backup node
2. Backup recieves the broadcast and setup the master/backup layer over TCP
3. Elevators recieve the broadcast and setup themselves over TCP

TCP as the main communication layer
1. Each packet are transmitted with receiving ACK and in order.
2. If elevators notice the master stop emitting on UDP, they may stop then wait for another master to take over.
3. If backup notice that master is down, it take over by starting emitting on UDP, but stops when master come back up.
> Rust std::net module to handle the setup/transmission part of TCP/UDP

> Communication with struct serialization

JSON seems to bring too much overhead and external libraries.
