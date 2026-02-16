<div align="center" width="100%">
    <img src="./frontend/public/icon.svg" width="128" alt="" />
</div>

# Dockru

A self-hostable Docker Compose management UI.

This project was forked from [Dockge](https://github.com/louislam/dockge) and the backend has been re-written in rust. The UI is still the original Vue UI (though there are plans to make changes).

## ⭐ Features

- 🧑‍💼 Manage your `compose.yaml` files
  - Create/Edit/Start/Stop/Restart/Delete
  - Update Docker Images
- ⌨️ Interactive Editor for `compose.yaml`
- 🦦 Interactive Web Terminal
- 🕷️ (1.4.0 🆕) Multiple agents support - You can manage multiple stacks from different Docker hosts in one single interface
- 🏪 Convert `docker run ...` commands into `compose.yaml`
- 📙 File based structure - Dockru won't kidnap your compose files, they are stored on your drive as usual. You can interact with them using normal `docker compose` commands

- 🚄 Reactive - Everything is just responsive. Progress (Pull/Up/Down) and terminal output are in real-time
- 🐣 Easy-to-use & fancy UI - If you love Uptime Kuma's UI/UX, you will love this one too

## 🔧 How to Install

Requirements:
- [Docker](https://docs.docker.com/engine/install/) 20+ / Podman
- (Podman only) podman-docker (Debian: `apt install podman-docker`)
- OS:
  - Major Linux distros that can run Docker/Podman such as:
     - ✅ Ubuntu
     - ✅ Debian (Bullseye or newer)
     - ✅ Raspbian (Bullseye or newer)
     - ✅ CentOS
     - ✅ Fedora
     - ✅ ArchLinux
  - ❌ Debian/Raspbian Buster or lower is not supported
  - ❌ Windows (Will be supported later)
- Arch: armv7, arm64, amd64 (a.k.a x86_64)

### Basic

Interactive mode (with prompts):
```
curl -sSL https://raw.githubusercontent.com/kyeotic/dockru/main/install | sudo bash
```

Non-interactive mode (with env vars):
```
curl -sSL https://raw.githubusercontent.com/kyeotic/dockru/main/install | \
  DOCKRU_DIR=/opt/dockru STACKS_DIR=/opt/stacks PORT=5051 sudo bash
```

## How to Update

```bash
cd /opt/dockge
docker compose pull && docker compose up -d
```

## Motivations

- I liked Dockge's simplified interface over Portainer, but the CPU usage while idle was noticeably worse than Portainer. I wanted to optimize it, and moving to Rust seemed like the best way to do that.

If you love this project, please consider giving it a ⭐.

## FAQ

#### "Dockru"?

The original name of Dockge was coined by its creator to sound like a meme work. I replaced the "ge" suffix with "ru" to represent that its written in Rust.

#### Can I manage existing stacks?

Yes, you can. However, you need to move your compose file into the stacks directory:

1. Stop your stack
2. Move your compose file into `/opt/stacks/<stackName>/compose.yaml`
3. In Dockru, click the " Scan Stacks Folder" button in the top-right corner's dropdown menu
4. Now you should see your stack in the list

#### Is Dockru a Portainer replacement?

Yes or no. Portainer provides a lot of Docker features. While Dockru is currently only focusing on docker-compose with a better user interface and better user experience.

If you want to manage your container with docker-compose only, the answer may be yes.

If you still need to manage something like docker networks, single containers, the answer may be no.

#### Can I install both Dockru and Portainer?

Yes, you can.

## Others

Dockru is built on top of [Compose V2](https://docs.docker.com/compose/migrate/). `compose.yaml`  also known as `docker-compose.yml`.
