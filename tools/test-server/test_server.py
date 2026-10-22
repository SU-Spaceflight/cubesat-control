# AI generated test server so I can test the rust websockets.
import asyncio
import json
import sys
import logging
import pathlib
from datetime import datetime

import websockets

def rotate_logs(log_dir="./server-log", max_logs=5):
    """Keeps only the most recent `max_logs` connection logs and deletes the rest."""
    path = pathlib.Path(log_dir)
    path.mkdir(exist_ok=True)

    # Grab all connection logs and sort them by modification time (oldest first)
    logs = sorted(path.glob("connection_*.log"), key=lambda p: p.stat().st_mtime)

    # Delete oldest logs until we are at or below our max_logs limit
    while len(logs) > max_logs:
        oldest = logs.pop(0)
        oldest.unlink(missing_ok=True)

def setup_connection_logger(conn_id):
    """Sets up a logger for a specific connection that outputs to both a file and the console."""
    log_dir = pathlib.Path("./server-log")
    log_dir.mkdir(exist_ok=True)

    log_filename = log_dir / f"connection_{conn_id}.log"

    # Create a unique logger for this connection
    logger = logging.getLogger(conn_id)
    logger.setLevel(logging.INFO)

    # Clear any existing handlers (just in case of ID collisions)
    logger.handlers.clear()

    # Formatter for our logs
    formatter = logging.Formatter('%(asctime)s - %(message)s')

    # File Handler
    fh = logging.FileHandler(log_filename)
    fh.setLevel(logging.INFO)
    fh.setFormatter(formatter)
    logger.addHandler(fh)

    # Console Handler (replaces our old print statements)
    ch = logging.StreamHandler(sys.stdout)
    ch.setLevel(logging.INFO)
    ch.setFormatter(formatter)
    logger.addHandler(ch)

    return logger

async def receive_telemetry(websocket, logger):
    """Listens for and spits out incoming telemetry from the Rust app."""
    try:
        async for message in websocket:
            data = json.loads(message)
            logger.info(f"[TELEMETRY RECEIVED] {data.get('dest')} at {data.get('uptime_ms', 0)}ms")
            logger.info("\n" + json.dumps(data.get("payload", {}), indent=2))
    except websockets.exceptions.ConnectionClosed:
        logger.info("Cubesat telemetry stream closed.")


async def send_commands(websocket, logger):
    """Simulates sending routed commands and keep-alives to the Pi."""
    test_commands = [
        {"dest": "gimbal", "cmd": {"trim_pan": 2.5}},
        {"dest": "gimbal", "cmd": {"trim_tilt": -8.0}},
        {"dest": "flight", "cmd": "yaw_right"},
    ]

    try:
        while True:
            for cmd in test_commands:
                await asyncio.sleep(3)
                logger.info(f"[COMMAND SENT] Routing to: {cmd['dest']}")
                await websocket.send(json.dumps(cmd))
            await asyncio.sleep(10)

    except websockets.exceptions.ConnectionClosed:
        logger.error("Cubesat disconnected. Halting commands.")


async def handle_cubesat(websocket):
    """Handles the connection lifecycle for a single Cubesat client."""
    # Generate a unique ID based on the current time
    timestamp_id = datetime.now().strftime("%Y%m%d_%H%M%S")

    # Setup logger and immediately rotate logs to ensure we only have 5
    logger = setup_connection_logger(timestamp_id)
    rotate_logs(max_logs=5)

    logger.info(f"--- New Cubesat connection established from {websocket.remote_address} ---")

    try:
        # Run both the receiving and sending loops concurrently for this connection
        await asyncio.gather(
            receive_telemetry(websocket, logger),
            send_commands(websocket, logger)
        )
    finally:
        # Cleanup logger handlers when the connection is eventually closed
        for handler in logger.handlers[:]:
            handler.close()
            logger.removeHandler(handler)


async def main():
    uri = "localhost"
    port = 8080

    # Print root startup messages directly to stdout/stderr since they aren't tied to a specific connection
    pathlib.Path("./server-log").mkdir(exist_ok=True)
    print(f"Starting GCS WebSocket Server on ws://{uri}:{port}...", file=sys.stderr)
    print("Waiting for Cubesat simulator to connect (Run forever mode)...", file=sys.stderr)

    async with websockets.serve(handle_cubesat, uri, port, ping_interval=5.0, ping_timeout=10.0):
        await asyncio.Future()  # Run forever


if __name__ == "__main__":
    asyncio.run(main())
