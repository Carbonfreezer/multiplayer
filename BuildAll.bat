@echo off

set OUT=deploy

echo Creating output directory...
if not exist %OUT% mkdir %OUT%

echo Building WASM...
cargo build -p tic-tac-toe --target wasm32-unknown-unknown --release

echo Building Server...
set CARGO_PROFILE_RELEASE_OPT_LEVEL=3
cargo build -p relay-server --release


echo Copying client files...

copy target\wasm32-unknown-unknown\release\tic-tac-toe.wasm %OUT%\
copy backbone-lib\web\*.js %OUT%\
copy games\tic-tac-toe\web\*.* %OUT%\

echo Copying server files...
copy target\release\relay-server.exe %OUT%\
copy relay-server\GameConfig.json %OUT%\


echo Done!
pause