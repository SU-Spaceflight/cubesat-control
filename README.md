# Payload
## Options for laser control
### 1: Run the system "blind", no laser tracking, just lookup table + testing and calibration
Pros:
- Little overhead
- Adjustments can be made on the fly using the manual trim control
Cons:
- Potentially not accurate
### 2: Track the laser using the regular camera
Pros:
- More accurate
- Less manual intervention required
Cons:
- More processing power required
- Laser may not be easily identifiable via optical camera
### 3: Track the laser using a IR camera and light pointed at the laser
Pros:
- Very accurate
- Less calibration
Cons:
- More power usage
- More computation required
- May not be allowed

## Challenges
- Proper servos and servo control
- Power usage
- Latency of program
- Processor overhead
- Camera distortion
- Laser projection from degrees onto flat wall
- Identification of the target despite distortion
- Identification of QR code (if used)
- Camera quality & FOV
- Identification of laser/IR light

# Comms
## Sent traffic (IC to server)
- General telemetry
- Camera feed
- Specific telemetry
## Recieved traffic (server to IC)
- Commands (rotate to x, trim to x,y, etc)
- Manual control if need be (emergency commands)
- Kill switches
- General commands (shutdown, restart, upload firmware)
## Protocols
- Open API for commands and telemtry
- Raw bitstream for camera feed
- Direct connection to host server
