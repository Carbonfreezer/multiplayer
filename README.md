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
This depot contains two JavaScript files that are part of other projects and are included here for completeness. These are:
1. **mq_js_bundle.js**: This is the marcoquad bundle that is needed to run macroquad as a WASM client. The source is [here](https://not-fl3.github.io/miniquad-samples/mq_js_bundle.js)
2. **sapp_jsutils.js**: This is part of the crate sapp_jsutils to work with JavaScript objects. The source of the script is [here](https://github.com/not-fl3/sapp-jsutils/tree/master/js)

# Getting started
To get everything running as fast as possible, clone this repository and compile it with *BuildAll.bat* on Windows and *BuildAll.sh* on Linux. On Linux, you have to make the shell script executable upfront. Once this is done, start the relay server in the deploy directory. This starts a web server on port 8080. Now type http://127.0.0.1:8080 into your favourite browser. You should see a room creation screen. Start a second browser window and do the same here, and you can play tic-tac-toe against yourself. Opening the same page in two tabs is problematic because you have to switch tabs a couple of times to send the messages. 
