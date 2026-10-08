# AI generated test server so I can test the rust websockets.
import asyncio
import websockets
import json

async def receive_telemetry(websocket):
    """Listens for and spits out incoming telemetry from the Rust app."""
    try:
        async for message in websocket:
            data = json.loads(message)
            print(f"\n[TELEMETRY RECEIVED] {data['dest']} at {data.get('uptime_ms', 0)}ms")
            print(json.dumps(data.get('payload', {}), indent=2))
    except websockets.exceptions.ConnectionClosed:
        print("Connection to Cubesat lost.")

async def send_commands(websocket):
    """Simulates sending routed commands and keep-alives to the Pi."""
    # Simulating the macro and micro control loop commands
    test_commands = [
        {"dest": "gimbal", "cmd": "trim_pan", "val": 2.5},
        {"dest": "flight", "cmd": "yaw_right"}
    ]

    # Send initial test commands
    for cmd in test_commands:
        await asyncio.sleep(3)
        print(f"\n[COMMAND SENT] Routing to: {cmd['dest']}")
        await websocket.send(json.dumps(cmd))

    # App-Level Keep-Alives: Ping every 2-5 seconds to survive network blackouts
    while True:
        await asyncio.sleep(4)
        ping = {"dest": "system", "cmd": "ping"}
        print(f"\n[KEEP-ALIVE SENT] {ping}")
        await websocket.send(json.dumps(ping))

async def main():
    # In production, this would be the Pi's IP address on the travel router network
    uri = "ws://localhost:8080"

    print(f"Attempting to connect to Cubesat at {uri}...")
    try:
        async with websockets.connect(uri) as websocket:
            print("Connected! Starting telemetry stream and command sequence...")
            # Run both the receiving and sending loops concurrently
            await asyncio.gather(
                receive_telemetry(websocket),
                send_commands(websocket)
            )
    except Exception as e:
        print(f"Connection failed: {e}. Is the Rust WebSocket server running?")

if __name__ == "__main__":
    asyncio.run(main())
