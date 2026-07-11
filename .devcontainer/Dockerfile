FROM debian:trixie

RUN apt-get update && apt-get install -y \
    build-essential \
    ca-certificates \
    curl \
    git \
    jq \
    libsqlite3-dev \
    libssl-dev \
    lsof \
    pkg-config \
    postgresql-client \
    sudo \
    unzip \
    wget \
    zsh \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -s /usr/bin/zsh monkesto \
    && echo "monkesto ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/monkesto \
    && chmod 0440 /etc/sudoers.d/monkesto

USER monkesto
WORKDIR /home/monkesto

ENV PATH="/home/monkesto/.cargo/bin:$PATH"

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path

RUN curl -L --proto '=https' --tlsv1.2 -sSf \
    https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

RUN cargo binstall cargo-make cargo-watch sqlx-cli --no-confirm

COPY zshrc /home/monkesto/.zshrc
