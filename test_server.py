# AI generated test server so I can test the rust websockets.
import asyncio
import json
import sys

import websockets


async def receive_telemetry(websocket):
    """Listens for and spits out incoming telemetry from the Rust app."""
    try:
        async for message in websocket:
            data = json.loads(message)
            print(
                f"\n[TELEMETRY RECEIVED] {data.get('dest')} at {data.get('uptime_ms', 0)}ms"
            )
            print(json.dumps(data.get("payload", {}), indent=2))
    except websockets.exceptions.ConnectionClosed:
        print("Cubesat telemetry stream closed.")


async def send_commands(websocket):
    """Simulates sending routed commands and keep-alives to the Pi."""
    test_commands = [
        {"dest": "gimbal", "cmd": {"trim_pan": 2.5}},
        {"dest": "flight", "cmd": "yaw_right"},
    ]

    try:
        while True:
            for cmd in test_commands:
                await asyncio.sleep(3)
                print(f"\n[COMMAND SENT] Routing to: {cmd['dest']}")
                await websocket.send(json.dumps(cmd))
            await asyncio.sleep(10)

    except websockets.exceptions.ConnectionClosed:
        print("Cubesat disconnected. Halting commands.")
        print("Cubesat disconnected. Halting commands.", file=sys.stderr)


async def handle_cubesat(websocket):
    """Handles the connection lifecycle for a single Cubesat client."""
    print(
        f"\n--- New Cubesat connection established from {websocket.remote_address} ---",
        file=sys.stderr,
    )
    print(
        f"\n--- New Cubesat connection established from {websocket.remote_address} ---"
    )

    # Run both the receiving and sending loops concurrently for this connection
    await asyncio.gather(receive_telemetry(websocket), send_commands(websocket))


async def main():
    uri = "localhost"
    port = 8080
    print(f"Starting GCS WebSocket Server on ws://{uri}:{port}...", file=sys.stderr)
    print(
        "Waiting for Cubesat simulator to connect (Run forever mode)...",
        file=sys.stderr,
    )

    async with websockets.serve(handle_cubesat, uri, port):
        await asyncio.Future()  # Run forever


if __name__ == "__main__":
    asyncio.run(main())
