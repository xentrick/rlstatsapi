#!/usr/bin/env python3

import asyncio
import websockets
import argparse

DEFAULT_URI = "ws://127.0.0.1:49123"


async def listen(uri):
    print(f"[INFO] Attempting to connect to {uri}...")

    try:
        async with websockets.connect(
            uri, open_timeout=5, ping_interval=10, ping_timeout=5
        ) as websocket:
            print("[SUCCESS] Connected!")

            while True:
                try:
                    print("[WAITING] Waiting for message...")
                    message = await asyncio.wait_for(websocket.recv(), timeout=10)
                    print(f"[RECEIVED] {message}")

                except asyncio.TimeoutError:
                    print(
                        "[TIMEOUT] No message received in 10 seconds (connection still alive)"
                    )
                except websockets.ConnectionClosed as e:
                    print(f"[CLOSED] Connection closed: {e}")
                    break

    except asyncio.TimeoutError:
        print("[ERROR] Connection attempt timed out")
    except ConnectionRefusedError:
        print("[ERROR] Connection refused (is the server running?)")
    except Exception as e:
        print(f"[ERROR] Unexpected error: {e}")


def main():
    parser = argparse.ArgumentParser(description="Simple WebSocket listener")
    parser.add_argument(
        "uri",
        nargs="?",
        default=DEFAULT_URI,
        help=f"WebSocket URI (default: {DEFAULT_URI})",
    )
    args = parser.parse_args()

    try:
        asyncio.run(listen(args.uri))
    except KeyboardInterrupt:
        print("\n[EXIT] Stopped by user")


if __name__ == "__main__":
    main()
