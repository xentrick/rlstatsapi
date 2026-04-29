# Overlay Setup Guide

This guide explains how to download the newest `rlstatsapi` release for your OS, configure Rocket League Stats API packet rate, and run the SOS broadcast server.

## 1) Download The Newest Release

1. Open the latest release page:
   - https://github.com/xentrick/rlstatsapi/releases/latest
2. Under **Assets**, choose the file matching your operating system and CPU.

### Windows

- Download the Windows release file `rlstatsapi-x86_64-pc-windows-msvc.zip`
- Extract it with Explorer or 7-Zip.

## 2) Configure Packet Send Rate (60)

Set this in your Rocket League Stats API INI file:

```ini
[TAGame.MatchStatsExporter_TA]
Port=49123
PacketSendRate=60
```

Typical Windows Steam path:

- `C:\Program Files (x86)\Steam\steamapps\common\rocketleague\TAGame\Config\DefaultStatsAPI.ini`

## 3) Start SOS Broadcast Server

On Windows, open the directory containing the ZIP file you downloaded. Then extract it.

Execute the `sos_broadcast` file by double clicking it and then navigate to the overlay web site.
