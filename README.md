# Introduction
This project contains a multi-player game system in Rust, primarily designed for browser based board games compiled as a WASM client. This project uses Axum/Tokio for the server, which also serves as a web server and a game-agnostic relay server. New games may even be added without
restarting the server. This is contained in the project [relay-server](#relay-server). Second, it includes a [library](#backbone-library) to construct multiplayer (browser) games on. It follows the philosophy of the client-hosted server, where clients can send a remote procedure call to the server, and the server
either sends delta updates or a whole view state to the clients. This is based on the network architecture of engines like Unity (NGO) or Unreal, though in a reduced form. This is contained in the library sub-project **backbone-lib**. The two components get interconnected
over web sockets. Shared protocol identifiers are kept in the sub-project [protocol](#protocol). As an example, a simple multiplayer game has been included in **games/tic-tac-toe**. You can find this system running in a more elaborate form on [Board-Game-Hub](https://board-game-hub.de).

# Why look at this project
Putting the central aspect aside, if you want to program multiplayer browser-based games, this project also contains some interesting solutions for problems I stumbled upon:

- If you are looking for a web socket app slightly more complex than the chat sample that comes with the tokio-axum project, this may be an interesting entry point.
- If you try to use web-sockets in combination with Macroquad and use quad-net, chances are, you might run into the same problems, as I did. Specifically, the version on crate.io cannot handle binary messages, and the non-WASM version also caused problems. 
The solution in [backbone-lib](#backbone-library) provides reduced WebSocket functionality that supports only binary messages, but runs in WASM and in native code. 
- The sample in [games/tic-tac-to](#tic-tac-toe) shows how to integrate egui with Macroquad and also how to fire the virtual keyboard, if the browser runs on a phone/pad. I am told that this solution does not work on Safari / Mac. 
If you have a solution for this problem, you are more than welcome to submit the correction.

In the following text, I would first like to cite some sources for JavaScript files I did not write myself, then comes a quick getting-started guide. Afterwards, the system's overall strategy is described, followed by a detailed explanation of its diverse components.

# Foreign sources
This depot contains two JavaScript files from other projects, included here for completeness. These are:
1. **mq_js_bundle.js**: This is the marcoquad bundle that is needed to run macroquad as a WASM client. The source is [here](https://not-fl3.github.io/miniquad-samples/mq_js_bundle.js).
2. **sapp_jsutils.js**: This is part of the crate sapp_jsutils to work with JavaScript objects. The source of the script is [here](https://github.com/not-fl3/sapp-jsutils/tree/master/js).

# Getting started
To get everything running as fast as possible, clone this repository and compile it with *BuildAll.bat* on Windows and *BuildAll.sh* on Linux. On Linux, you have to make the shell script executable upfront. Once this is done, you can start the relay server in the deploy directory. This starts a web server on port 8080. Now type http://127.0.0.1:8080 into your favourite browser. You should see a room creation screen. Start a second browser window and do the same here, and you can play tic-tac-toe against yourself. Opening the same page in two tabs is problematic because you have to switch tabs a couple of times to send the messages. 

# General overview
The overall architecture and idea of the system are sketched in the following image:

![Architecture](Layout.png)

The system contains the following components:

* **Relay-Server**: The game agnostic server, that has functionality for room and connection management, it is game agnostic. The main services it
    provides forwarding of remote procedure calls from clients to the server and sending partial updates and complete View State changes to clients.
* **View State**: Essentially a data structure that is controlled by the client-hosted server and sent to the clients. It may be sent entirely or as a series  
  of partial updates. The client typically receives a complete update upon joining the room or when the client-hosted server decides to do so, 
  which typically happens at the start of the new game. The Frontend may use partial View State updates to display transition animations.
* **Backend**: This contains the real game logic and is entirely event-based, as the system has been primarily designed for board games in mind.
  The backend resides solely on the client-hosted server side and must implement the **BackEndArchitecture** trait. The backend has an internal
  view state. All incremental changes are logged in a *BackendCommand* vector and also need to get applied to an internally administrated
  view state, which may get sent over the network if required.
* **Middle Layer**: This is the central part of the library **backbone-lib**. The middle layer receives requests from the front end and sends requests to the 
  backend on the server side. To have a clear chain of command and to avoid any confusion with smart pointers and RefCells, the backend does
  not send any commands to the middle layer, but builds a command buffer. This buffer may get polled from the middle layer. The middle layer
  is the nexus for all the information flow in the system. It also handles new players joining the client-hosted server, providing them with a full view-state update.
* **Frontend**: This is the main program written in Macroquad, which is based on a core game loop. It has to heartbeat the middle layer, takes care
  of the initial game connection, and can send game mechanics-relevant input over an RPC. Then it can poll state changes from the middle layer to either 
  hard-set the view state or perform animation transitions.

A more specific, detailed documentation gets generated when you run *cargo doc*, which is done automatically when you run the 
script in [Getting started](#Getting-started).

# Workspace description
In the following subsections, we will describe the various members of the workspace in more detail. The **BuildAll** script in
the root directory will copy the relevant compiled results and accompanying files from the diverse sources to the deploy directory as 
explained in [Getting started](#getting-started).

## Protocol
This library project contains some shared definitions between the relay server and the backbone library. As every message is marked with a byte header, the meaning of those headers and, to some extent, the message sizes are encoded in constants here. 
The structure **JoinRequest** contains the protocol information for a client to join a game via the relay server.

## Relay Server
When the relay server starts, it listens on port 8080. For practical deployment purposes, it is advisable to put it behind 
a reverse proxy like Caddy. 
The relay server loads a JSON file **GameConfig.json** on startup that contains the information on which games exist and what the 
maximum number of players a room should hold. Setting this value to 0 means that there is no limitation.
A simple JSON file looks like this:
````
[
  {
    "name" : "tic-tac-toe",
    "max_players" : 10
  }
]
````
More games may be added by extending the array. Once the server is running, the list of games may be extended during runtime.
This may be done by calling the **reload** site with the browser on the domain where the relay server is running. 
The site **enlist** shows the currently active rooms.

The overall idea of the relay server is that two tokio tasks are servicing each connected client. The logic is split on the highest
level, whether the connection belongs to the client-hosted server or a client. These tasks refer to internal communication channels
that have been set up before in the handshake phase. These channels belong to a room (see **server_state**). This is an mpsc sender
to send messages from the clients to the client-hosted game server, and a broadcast sender the other way around. As only new clients need
a full update of the view state, this decision is taken care of in the **send_logic_client** method.

To keep the relay server as game-agnostic as possible, only connection and disconnection processing is done here. Otherwise, 
it passes on information for Client to Server RPCs, where only the player ID gets attached. In the reverse direction, it can kick a player,
or send partial updates, full updates, or reset. A lot of error handling and tracing is done here, with error messages sent to the clients
before closing the connection.

## Backbone Library
The backbone library contains in its web folder two JavaScript files, that become relevant when a WASM module gets compiled.
This is the Macroquad library as mentioned [Foreign Sources](#foreign-sources) and a miniquad plugin to take care of the relevant web socket 
implementation. The web socket implementation is limited here by having only one web socket at the time and by only sending and receiving
binray messages. This is handled by the file **quad_ws.js** both files must be included in a web page, that is using the compiled
WASM plugin. The remaing relevant JavaScript files and a sample web page are shown in the web directory of [Tic-Tac-Toe](#tic-tac-toe).

The Rust part of the web socket implementation can be found in **web_socket_iterface.rs**, where the first part of the file is
essentially abstracting over the relevant parts of the web socket functionality by using **ewbsock** in the non WASM part and 
the own implementation in the WASM part. If you like to do a web socket implementation in a WASM context this may the point 
to take a closer look at. 

It provides communication and connection functionality separated again for the case that we are a client hosted server or a pure 
client. Sending is done immediately and receiving is done on a polling basis. This should be performed in the heartbeat of the 
game core loop and takes into account the fact, that we can not run threads easily in a non WASM environment.

The module **traits** contains the trait **BackEndArchitecture**, that has to be implemented by the application. The core 
logical functionality of the library is contained in **middle_layer**. These are the two modules mentioned in [General Overview](#general-overview).

The **middle_layer** comes with a bare bone sample documentation of how a game should be structured. A more detailed
example of this can be found in the section of [Tic-Tac-Toe](#tic-tac-toe).The purpose of the middle layer is essentially a 
logistiacal one for passing messages between the frontend, the backend and the relay server around. On top of this it interafaces
with a timer system. The timer system has been added, because the backend, that has to be implemented by the game is purely
event driven. The timer functionality is contained in the module **timer**.



## Tic-Tac-Toe


egui and hidden text field trick.
