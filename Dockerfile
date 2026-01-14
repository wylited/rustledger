# Multi-stage build for rustledger
# Produces a minimal image with static musl binaries

# Build stage
FROM rust:1.83-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build
COPY . .

RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage - scratch for minimal size
FROM scratch

# Copy all CLI binaries
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-check /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-format /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-query /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-report /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-doctor /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-extract /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/rledger-price /usr/local/bin/

# Bean-* compatibility aliases
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-check /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-format /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-query /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-report /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-doctor /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-extract /usr/local/bin/
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bean-price /usr/local/bin/

# Default to rledger-check
ENTRYPOINT ["rledger-check"]
