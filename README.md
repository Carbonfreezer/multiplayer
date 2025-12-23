# Introduction
This project contains a multi-player game system in Rust, primarily designed for Browser Games compiled as a WASM client. This project uses Axum/Tokio for the server, which also serves as a web server and a game-agnostic relay server. New games may even be added without
restarting the server. This is contained in the project **relay-server**. Second, it includes a library to construct multiplayer (browser) games on. It follows the philosophy of the client-hosted server, where clients can send a remote procedure call to the server, and the server
either sends delta updates or a whole view state to the clients. This is based on the network architecture of engines like Unity (NGO) or Unreal, though in a reduced form. This is contained in the library sub-project **backbone-lib**. The two components get interconnected
over web sockets. Shared protocol identifiers are kept in the sub-project **protocol**. As an example, a simple multiplayer game has been included in **games/tic-tac-toe**. You can find this system running in a more elaborate form on [Board-Game-Hub](https://board-game-hund.de).

# Why look at this project
Putting the central aspect aside, if you want to program multiplayer browser-based games, this project also contains some interesting solutions for problems I stumbled upon:

- If you are looking for a web socket app slightly more complex than the chat sample that comes with the tokio-axum project, this may be an interesting entry point.
- If you try to use web-sockets in combination with macroquad and use quad-net, chances are, you might run into the same problems, as I did. Specifically the version on crate.io can not deal with binary messages and the non WASM version also made problems. 
The solution in **backbone-lib** provides reduced WebSocket functionality that supports only binary messages, but runs in WASM and in native code.
- The sample in **games/tic-tac-toe** shows how to integrate egui with Macroquad and also how to fire the virtual keyboard, if the browser runs on a phone/pad. I am told that this solution does not work on Safari / Mac. 
If you have a solution for this problem, you are more than welcome to submit the correction.

In the following text, I would first like to cite some sources for JavaScript files I did not write myself, then comes a quick getting-started guide. Afterwards, the system's overall strategy is described, followed by a detailed explanation of its diverse components.  
