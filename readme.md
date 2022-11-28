# Mouse/Keyboard Hook Debugger

This is a small utility to try and determine what programs may be running on
your machine that are hooking your mouse and/or keyboard input events.

This may be useful if you are seeing strange mouse/keyboard behavior on your machine.

# Building and running

Just download the code using Git and use "cargo run" to run the program. It will create a file
in the user temporary directory named "input_events.txt". This file will contain all the
interesting things.

**Note:** The user temporary directory can be reached by hitting "Window Key + R" and
typing "%temp%" into the Run dialog.

