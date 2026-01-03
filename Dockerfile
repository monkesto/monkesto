# Get started with a build env with Rust nightly
FROM rustlang/rust:nightly-bookworm as builder

# Install required tools
RUN apt-get update -y \
  && apt-get install -y --no-install-recommends clang


RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs

RUN npm install tailwindcss @tailwindcss/cli

# Make an /app dir, which everything will eventually live in
RUN mkdir -p /app
WORKDIR /app
COPY . .

# Build the app
RUN make build

FROM debian:bookworm-slim as runtime
WORKDIR /app
RUN apt-get update -y \
  && apt-get install -y --no-install-recommends openssl ca-certificates \
  && apt-get autoremove -y \
  && apt-get clean -y \
  && rm -rf /var/lib/apt/lists/*

# Copy the server binary to the /app directory
COPY --from=builder /app/target/release/monkesto /app/

# /target/site contains our JS/WASM/CSS, etc.
COPY --from=builder /app/target/site /app/site

# Copy Cargo.toml if it’s needed at runtime
COPY --from=builder /app/Cargo.toml /app/

# Set any required env variables
ENV RUST_LOG="info"
ENV SITE_ADDR="0.0.0.0:8080"
ENV SITE_ROOT="site"
EXPOSE 8080

# Run the server
CMD ["/app/monkesto"]
