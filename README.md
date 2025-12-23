# Introduction
This project contains a multi-player game system in Rust, primarily designed for Browser Games compiled as a WASM client. This project uses Axum/Tokio for the server, which also serves as a web server and a game-agnostic relay server. New games may even be added without
restarting the server. This is contained in the project **relay-server**. Second, it includes a library to construct multiplayer (browser) games on. It follows the philosophy of the client-hosted server, where clients can send a remote procedure call to the server, and the server
either sends delta updates or a whole view state to the clients. This is based on the network architecture of engines like Unity (NGO) or Unreal, though in a reduced form. This is contained in the library sub-project **backbone-lib**. The two components get interconnected
over web sockets. Shared protocol identifiers are kept in the sub-project **protocol**. As an example, a simple multiplayer game has been included in **games/tic-tac-toe**. You can find this system running in a more elaborate form on [Board-Game-Hub](https://board-game-hub.de).

# Why look at this project
Putting the central aspect aside, if you want to program multiplayer browser-based games, this project also contains some interesting solutions for problems I stumbled upon:

- If you are looking for a web socket app slightly more complex than the chat sample that comes with the tokio-axum project, this may be an interesting entry point.
- If you try to use web-sockets in combination with macroquad and use quad-net, chances are, you might run into the same problems, as I did. Specifically, the version on crate.io cannot handle binary messages, and the non-WASM version also caused problems. 
The solution in **backbone-lib** provides reduced WebSocket functionality that supports only binary messages, but runs in WASM and in native code.
- The sample in **games/tic-tac-toe** shows how to integrate egui with Macroquad and also how to fire the virtual keyboard, if the browser runs on a phone/pad. I am told that this solution does not work on Safari / Mac. 
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
  is the nexus for all the information flow in the system. It also takes care of new players arriving on the client-hosted server, providing them with a full view-state update.
* **Frontend**: This is the main program written in Macroquad, which is based on a core game loop. It has to heartbeat the middle layer, takes care
  of the initial game connection, and can send game mechanics-relevant input over an RPC. Then it can poll state changes from the middle layer to either 
  hard-set the view state or perform animation transitions.

A more specific, detailed documentation gets generated when you run *cargo doc*, which is done automatically when you run the script in [Getting started](#Getting-started).

# Detailed descriptions
In the following subsections we will describe the different members of the workspace in a bit more detail.

## Relay Server

## Backbone Library

Websocket implementation trick.

## Tic-Tac-Toe


egui and hidden text field trick.