// quad_ws.js - Minimal WebSocket plugin for Miniquad (quad-net replacement)
// Binary messages only, polling-based


"use strict";

let ws = null;
let ws_connected = false;
let incoming_queue = [];

miniquad_add_plugin({
    name: "quad_ws",
    version: "0.1.0",

    register_plugin: function(importObject) {

        
        // Connect to WebSocket server
        importObject.env.quad_ws_connect = function(url_ptr, url_len) {
            // UTF8 decoding inline, wasm_memory ist jetzt verfügbar
            const bytes = new Uint8Array(wasm_memory.buffer, url_ptr, url_len);
            const url = new TextDecoder().decode(bytes);

            if (ws !== null) {
                ws.close();
            }
            
            ws_connected = false;
            incoming_queue = [];
            
            try {
                ws = new WebSocket(url);
                ws.binaryType = "arraybuffer";
                
                ws.onopen = function() {
                    ws_connected = true;
                };
                
                ws.onclose = function() {
                    ws_connected = false;
                    ws = null;
                };
                
                ws.onerror = function() {
                    // Error triggers onclose, nothing extra needed
                };
                
                ws.onmessage = function(event) {
                    if (event.data instanceof ArrayBuffer) {
                        incoming_queue.push(new Uint8Array(event.data));
                    }
                };
            } catch (e) {
                ws_connected = false;
                ws = null;
            }
        };
        
        // Check if connected
        importObject.env.quad_ws_connected = function() {
            return ws_connected ? 1 : 0;
        };
        
        // Send binary data
        importObject.env.quad_ws_send = function(data_ptr, data_len) {
            if (ws !== null && ws_connected) {
                const data = new Uint8Array(wasm_memory.buffer, data_ptr, data_len);
                ws.send(data.slice().buffer);
            }
        };
        
        // Check if message is available
        importObject.env.quad_ws_has_message = function() {
            return incoming_queue.length > 0 ? 1 : 0;
        };
        
        // Get next message length (0 if none)
        importObject.env.quad_ws_next_message_len = function() {
            if (incoming_queue.length === 0) return 0;
            return incoming_queue[0].length;
        };
        
        // Read next message into buffer, returns actual length
        importObject.env.quad_ws_recv = function(buffer_ptr, buffer_len) {
            if (incoming_queue.length === 0) return 0;
            
            const msg = incoming_queue.shift();
            const copy_len = Math.min(msg.length, buffer_len);
            
            const dest = new Uint8Array(wasm_memory.buffer, buffer_ptr, copy_len);
            dest.set(msg.subarray(0, copy_len));
            
            return msg.length; // Return actual length (caller can detect truncation)
        };
    }
    
});

