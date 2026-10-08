I'm writing this proof of concept assuming that all the systems are running on a single raspberry pi.

# Current plans
## Tracking and Aiming
- The camera feed will be analysed for 3 nested shapes, with an ellipse fitted to the inner one. 
  - The centre of that elipse will be the aim point.
- If the calculated aim point is outside the central (parameter `X=50`) `X`% of the feed, we ask the satellite to rotate until it is within the central `X`%. This means that once the target is roughly centered, the camera distortion will be minimised, leading to more accuracy. 
  - This might not be neccesary, or maybe only for extreme angles, testing is needed. If it works well enough without needing to rotate the satelite, that's better.
- At the same time, the gimbal will be aiming (and hopefully scoring points). 
- If the laser is not detected on through the camera, we project a ray to the wall based on the angle of the satellite, and aim there. This should hopefully get the laser within a degree or two of the target, which should be accurate enough for full points.
  - This is the default state of the targetting.
- If the laser is easily visible to the camera, we simply use a PID loop to move the pixel coordinate of the laser to the pixel coordinate of the target.
  - This should hopefully be more accurate than the projection based approach, and will be the preferred method if possible.
  - We can have a confidence metric for detecting the laser, so that it only uses the laser PID if it's very sure that it knows where the laser is.
- Failure cases:
  - If the target is not visible to the camera, and the CV is failing, we can rotate the satellite to try and spot the target if it's off screen, or get a better angle if the CV is failing
  - If the CV is still failing, we can have a manual override available that allows a human at aground control to set the target point in the camera.
  - If the targetting is failing, we can have a manual override availa that allows a human to manually move the gimbal using a virtual joystick.
    - Obviously, having the automated systems fail and relying on human input will be slower and wil get us less points, but it's better than no points if something goes wrong.
- During building, we need to calibrate the camera's distortion. We can do that by making a calibration plate and running the camera through a calibration sequence and saving the results.
- We can also detect the QR code and use that to get the general location of the target.

## Comms
### Transmitted data
- Telemetry data via JSON WebSocket
  - Sensor readouts
  - Internal senstors like PI temperature
  - System health checks
  - Payload and targetting information
  - Forwarded telemetry from the flight computer
- Raw camera feed over RTSP

### Recieved data
- Commands via JSON WebSocket
  - Trim commands in case of drifting control loops
  - General commands (e.g. rotate to x degrees)
  - Manual override commands (e.g. aim payload at x, y)
  - Kill commands (e.g. shut down pwer to servos)
  - "Meta" commands (e.g. restart, upload new program)


In the case that the WiFi connection is struggling, we can switch from JSON to either ProtoBuf or MessagePack. Both pack the data much more efficiently; ProtoBuf requires an extra compilation step on the sat and server, MessagePack is plug and play.

## Challenges
- Proper servos and servo control
- Power usage
  - Might need a seperate driver board for servos if they jitter due to power fluctuations
  - We probably will need to use a seperate power source for the servos, as they might draw enough current from the PI to cause it to crash and restart
- Latency of program
- Processor overhead
- Camera distortion
  - Hopefully solved by calibration step
- Identification of the target despite distortion
- Identification of QR code (if used)
- Camera quality & FOV
  - Also what compression settings and downscaling to use for CV and sending the camera data
- Identification of laser

## Other
PIs use network manager, ehich is pretty good about reconnecting when they lose signal. We can force it to be more aggressive and wait longer to timeout:
```
autoconnect=true
autoconnect-retries=0
```
The satellite should use a watchdog service to ping the server directly, if it can't find it for X number of tries, then restart the wifi service on the RPI by `sudo ip link set wlan0 down && sudo ip link set wlan0 up`. This will be just in case network manager completely fails and we need to do a hard reset to regain connection.

When the system boots up, a systemd job will run the program, and restart it if it crashes. Something like this:
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
