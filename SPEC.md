# Preamble

I will call the Rust program the satellite and the python program the GC (ground control) from now on.

# Structure

The program is split into the following files, with the following roles:

- `main.rs` - The entrypoint of the program. Initialises logging and every Tokio task.
- `arbiter.rs` - Manages local commands. When a system wants to move a gimbal, it sends it as a command that is executed by the arbiter. Remote commands are also forwarded to the arbiter.
- `commands.rs` - Defines commands that the system can receive from GC. Rust's [serde](https://serde.rs/) allows me to define how JSON is translated into Rust structs and enums.
- `control.rs` - Handles the PID controller and targeting computer. The targeting computer takes the data from the CV pipeline and makes servo commands for the arbiter to execute.
- `hardware.rs` - Where all the connections to external hardware components will go. Currently, data is randomly generated, and outputs like the servos just report when they are told to do.
- `protocol.rs` - Handles incoming commands from the GC, as well as holding the structure of the outgoing telemetry packets.
- `vision.rs` - Holds the CV pipeline for identifying the target and laser.

# Communication

The GC serves a websocket at `127.0.0.1:8080`. The satellite tries to connect to this websocket, retrying every three seconds if it cannot make a connection. In the actual implementation, this would connect to the actual server's IP and port over the wifi link between satellite and GC.

Data is sent via the websocket in the form of JSON. In future, we could look into using a packed binary format like [ProtoBuf](https://protobuf.dev/) or [MessagePacket](https://msgpack.org/index.html) if we need the extra bandwidth.

## JSON Spec

This is subject to change, I just made something to start testing with, with the intention of changing it in future.

### Data from the satellite to the GC

```json
{
  "uptime_ms": 8467,
  "type": "cpu_temp_packet",
  "payload": {
    "cpu_temp_c": 30.40753233
  }
}
```

Another example:

```json
{
  "uptime_ms": 75983,
  "type": "tele",
  "payload": {
    "target": {
      "x": 1049.5317,
      "y": 462.37814
    },
    "laser": {
      "x": null,
      "y": null
    }
  }
}
```

The telemetry data has an uptime in milliseconds. As the Raspberry Pi does not have a way to track datetime when the power is off, using uptime is the only real option. The `dest` field likely is not used on the GC side, and relates to the internal Rust representation.

The `payload` field stores the actual data, in this case the temperature and the predicted location of the target center and laser location. If the system cannot find the target or laser, those values will be null instead.

### Data from the GC to the satellite

```json
{
  "dest": "gimbal",
  "cmd": {
    "manual_override": {
      "pan": 12.5,
      "tilt": 0.1
    }
  }
}
```

Another example:

```json
{
  "dest": "flight",
  "cmd": "yaw_right"
}
```

The commands sent to the satellite have a simple syntax where `dest` determines where the command is routed to, and `cmd` indicates what is to be done. If one or more fields are supplied in the `cmd`, they are used as parameters for the command.

# Hardware

- Camera

  The fake camera feed is created by placing `target.png` on a blank canvas and having it bounce around like a DVD logo, squashing as it moves to simulate planar distortion. A laser is created by having a red circle move in a large circle around the canvas. To test the different targeting algorithms, it disappears and reappears on a cycle.

- Temperature

  Currently just returns a random value. Will get the value from the Pi's internal temperature sensor. Eventually we can add other temperature measurements like an external temperature measurement probe and temperature readouts from the flight computer.

- Servos

  Servos currently just log their current angle, pulse width and duty cycle.

# Interboard communication

I haven't started looking into this, but I assume that this board will handle all commands, and will send/forward relevant commands to the flight board, and forward telemetry from the flight board to the GC. We will need to look into how this communication will be done.

# Async

Async programming is required because the system needs to handle communication both ways, as well as manage a CV pipeline, control servos, and record telemetry data. For this purpose I am using Rust with the [Tokio](https://tokio.rs/) runtime. I am using this combination because it is safe, fast and proven.

## Tokio runtime

The program relies primarily on Tokio tasks and channels. Tokio tasks run asynchronously, and by adding a `loop {}` within the task, I can make a system that runs and constantly processes/moves/creates data.

## Channel Architecture

For communication between threads, I am using two channel types, `mpsc` and `watch`. `mpsc` channels are used where multiple tasks pass in data to be consumed by one task. For example, the telemetry channel has multiple providers that pass in messages to the channel, which is then read directly and passed to the websocket by the telemetry handler.

`watch` channels just hold the most recent thing passed into them, discarding any old values. This is useful when a task needs to directly pass data to another task. For example, the camera feed will come in at something like 30 fps (one frame per 33ms), and is added to a watch channel. The CV pipeline reads from that channel and passes the data on.

If the CV falls behind, the frames don't stack up, and the pipeline always gets the newest frame to process. Additionally, other tasks can read from the watch output without consuming it, allowing for a separate system to look at a channel periodically for telemetry purposes. This is how the CV output data is sent to the GC; a task peeks at the CV output channel every X seconds and sends the data to the telemetry channel without disturbing the pipeline.

# Redundancy

## Pi level

When it comes to actually running the system on the Pi, it needs to be able to:

1. Run at startup
2. Re-run if it crashes
3. Restart networking if it fails

To this end, the program can run using a systemd service, which will allow for autostarting and re-running on crashes.

```
[Unit]
Description=SatControl Targeting System
After=multi-user.target

[Service]
Type=simple
User=pi
ExecStart=/home/pi/satcontrol/target/release/satcontrol
WorkingDirectory=/home/pi/satcontrol

Restart=always
RestartSec=1
StartLimitIntervalSec=0

Nice=-10
IOSchedulingClass=realtime

[Install]
WantedBy=multi-user.target
```

On Linux, repeated failures to connect to the GC WiFi can cause the networking daemon to temporarily suspend connection attempts. We can set Network Manger to try for as long as possible with the following configuration:

```
autoconnect=true
autoconnect-retries=0
```

But it will still eventually fail.

To prevent this, and to handle any runtime bugs in the system networking, I have a watchdog running. If the watchdog does not see a connection to the GC for a certain amount of time, currently it just logs that it ran out of time.

I am planning to make the following happen when the watchdog times out:

1. Reset the connection within the program
2. Restart the system connection
3. Restart the system networking daemon

Each step will have some timeout associated with it to see if the connection is re-established.

## Program level

Within the program, I have used Rust's error handling to try and ensure that the program never crashes aside from severe issues like a hardware fault or an out of memory error. It still needs to be improved, but I believe that I can get to a state with sufficient redundancy.

I am making use of the logging crate (library) [tracing](https://crates.io/crates/tracing) to provide logging levels written to the console as well as to a rotating log file. The log file has all logs at or above `debug` level written to it, and the logs written to the console are defined by the environment variable `RUST_LOG`.

When the feature flag `save_debug_frames` is enabled, the output of the CV pipeline is written as a series of images to `./debug_frames`. They have the estimated target position drawn on them.

# Computer vision & targeting

## CV

The CV pipeline follows these steps:

1. Reads the current frame from the channel and converts it to grayscale.
2. Runs canny edge detection to find all contours.
3. Matches ellipses to every contour it can that is larger than a certain size (to prevent matching on the QR code or other small shapes).
4. Calculates the center of each ellipse, and if two centers are close enough it is assumed that that point is the center.
   - We will need to do real-world testing with an actual camera feed to determine if this is good enough, or if we need to refine the CV algorithm.

5. Evaluates the target's movement distance since the last frame. If it is below a certain threshold, a smoothing function is applied. If it exceeds the threshold, the jump is allowed, assuming the target has moved instantly."

## Targeting

If the target and laser are visible, the targeting computer will use the PID to move the laser to the target location. If the laser disappears from view, the computer will continue to use the PID for a certain TTL (time to live). This prevents thrashing the servos with differing commands if the laser keeps dropping in and out of view.

If the TTL expires while the laser is hidden, the system calculates the required gimbal vectors using basic geometry. Both methods will need to be tuned for the specific hardware configuration we go with, and I need to add the camera distortion compensator into the pipeline.
