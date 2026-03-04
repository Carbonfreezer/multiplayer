@echo off

set OUT=deploy

echo Building Server...
set CARGO_PROFILE_RELEASE_OPT_LEVEL=3
cargo zigbuild -p relay-server --release --target x86_64-unknown-linux-gnu



echo Done!
pause