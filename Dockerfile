# Stage 1: Build reach-supervisor
FROM rust:1.94-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY crates/ crates/

RUN cargo build --release -p reach-supervisor -p reach-browserd

# Stage 2: Runtime
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV DISPLAY=:99
ENV HOME=/home/sandbox

# Layer 1: System deps — display, VNC, window manager, X11 tools, network
# debug tooling. The network tools (iproute2, net-tools, dnsutils, jq) are
# load-bearing for any agent that needs to introspect ports, sockets, or
# DNS from inside the sandbox. Without them, debugging "is chrome's CDP
# port actually listening?" requires apt-get inside a running container,
# which is slow and bloats the layer.
RUN apt-get update && apt-get install -y --no-install-recommends \
    xvfb \
    x11vnc \
    openbox \
    xdotool \
    scrot \
    xclip \
    fonts-noto-core \
    fonts-noto-mono \
    dbus-x11 \
    curl \
    wget \
    ca-certificates \
    gnupg \
    iproute2 \
    net-tools \
    dnsutils \
    iputils-ping \
    jq \
    procps \
    less \
    python3 \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Layer 2: Chrome dependencies (Playwright will provide the binary)
RUN apt-get update && apt-get install -y --no-install-recommends \
       libnss3 libatk1.0-0t64 libatk-bridge2.0-0t64 libcups2t64 libdrm2 \
       libxkbcommon0 libxcomposite1 libxdamage1 libxrandr2 libgbm1 \
       libpango-1.0-0 libcairo2 libasound2t64 libxshmfence1 \
    && rm -rf /var/lib/apt/lists/*

# Layer 3: Node.js + computer-use-mcp
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g computer-use-mcp \
    && rm -rf /var/lib/apt/lists/*

# Layer 4: Playwright + Scrapling + websockify
ENV PLAYWRIGHT_BROWSERS_PATH=/opt/ms-playwright
RUN pip install --break-system-packages \
    playwright \
    "scrapling[fetchers]" \
    websockify \
    && playwright install chromium \
    && scrapling install \
    || true
    
RUN ln -sf $(find /opt/ms-playwright/chromium-*/chrome-linux -name chrome -type f | head -n 1) /usr/bin/google-chrome-stable \
    && ln -sf $(find /opt/ms-playwright/chromium-*/chrome-linux -name chrome -type f | head -n 1) /usr/bin/google-chrome

# Layer 5: noVNC
RUN git_url="https://github.com/novnc/noVNC.git" && \
    apt-get update && apt-get install -y --no-install-recommends git && \
    git clone --branch v1.5.0 --depth 1 $git_url /opt/noVNC && \
    git clone --branch v0.12.0 --depth 1 https://github.com/novnc/websockify /opt/noVNC/utils/websockify && \
    apt-get purge -y git && apt-get autoremove -y && rm -rf /var/lib/apt/lists/*

# Layer 6: reach-supervisor binary
COPY --from=builder /build/target/release/reach-supervisor /usr/local/bin/reach-supervisor
COPY --from=builder /build/target/release/reach-browserd /usr/local/bin/reach-browserd

# Layer 7: User + permissions + X11 socket dir
RUN useradd -m -s /bin/bash sandbox \
    && echo "sandbox ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers \
    && mkdir -p /tmp/.X11-unix \
    && chmod 1777 /tmp/.X11-unix

# Chrome policies — disable auto-updates, first-run
COPY config/chrome-policies.json /etc/opt/chrome/policies/managed/reach.json

# Openbox config
RUN mkdir -p /home/sandbox/.config/openbox
COPY config/openbox-rc.xml /home/sandbox/.config/openbox/rc.xml
RUN chown -R sandbox:sandbox /home/sandbox

USER sandbox
WORKDIR /home/sandbox

EXPOSE 5900 6080 8400

ENTRYPOINT ["reach-supervisor"]
