This is a small documentation to explain the purpose of every directory/module of the project.
First of all, most of the code is lacking comment, this is bad, and we are striving to fix that.

./bin contains the simulator and the hardware server.

./readmeFiles is a bunch of files/documentation, but it's not that relevant for the moment.

./faulted is a module used to control/fault the entire program, it is used mostly to stop threads properly by changing a
common variable to true.

./log is, as indicated by the name, a logging module, it is specifically made to work over the network. I would recommend not
to read it since it doesn't bring anything for the scope of the project besides logging capacities.

**./common** contains mostly network code for the moment.\
**./common/messages.rs** specifies every possible messages that can be passed over the network.\
**./common/data_struct.rs** specifies a bunch of structures used to represents state.\
**./common/connection.rs** specifies a lot of network related constants such as timeout, send period, etc...\
**./common/connection/connection_handle** is the core of the module. An handle represent a connection-aware, customizable
wrapper around a connection. It serves multiple goals and can be used by every part of the code that needs message passing over the network
It comes with a lot of features by default: Signalizing when connection is established or lost, automatic sending of keep-alive messages. Auto-encoding from and to 
the Message data type. Detecting when the connection become too unstable and more. \
You can easily add behavior to a handle. In this project, we use a listener over UDP to determine which controller is the master and
a client is able to transition to the new master without needing to do anything. Authentication can also be used to identify every handle.
You can take a look at client_init and backup_init to see how listening over udp is done. \
**./common/connection_init** contains server code used mostly to broadcast UDP frames and start a TCP listening daemon capable
of identifying client as either elevator or other controller/backup.\
**./common/client_pool** contains code used to aggregate the messages of every elevator into one single channel
for the controller to use.\
**./common/synchronisation** contains code used to handle the synchronisation of master/backup, reconciliation after a failure and automatic reconnection over TCP using the
same mechanism of broadcasting the address of UDP. In particular, synchronisation event should be handled solely in pairing.rs
This file allow to receive every Message and react to them in an event-driven way.


**./process** is the core of the program, it contains both elevator(client) and master/backup(controller) code\
**./process/main.rs** is the entrypoint of the program and handler of the arguments.\
**./process/process** contains the entrypoint of the both the client and controller subprocess.
**./process/elevator** contains the actual logic of the client and the controller.
In particular, elevator_hardware.rs is used to communicate with the elevator or the simulator and also received events form it\
door_control.rs is a small file used to handle door obstruction and automatic closing of the door by sending events when the door close.
In controller, light_control is used to translate some data structure to light control structure, used to light up and down the button.
elevator_pool is an extension of client_pool, used to link the elevator state (request queue) with the elevator client.\
elevator_service represents a set of request that can be handled by only moving in a single direction, they can be chained in a queue.
elevator_state is used to respond to most event and contains the actual event loop for the controller.