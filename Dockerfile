FROM rust:slim-buster@sha256:bbf94ba964b3d9a47600d8f5e2275fc461df300300216dc9f173e42e46300f74 as builder

ENV SOLVERSION=1.6.8
ENV SOLINSTALLCHECKSUM=be0d60ba830b9f4910faa3d7095f7ff3666392d501af37720cf6d6d0704e6019
ENV SOLPATH="/root/.local/share/solana/install/active_release/bin"
ENV SOLIDOBUILDPATH="$SOLPATH/solido-build"
ENV SOLIDORELEASEPATH="$SOLPATH/solido"

# Install Solana tools
RUN apt -y update \
    && apt -y install curl clang lld libudev-dev libssl-dev pkg-config libhidapi-dev libjemalloc-dev \
    && curl -sSfLO https://release.solana.com/v$SOLVERSION/install \
    && echo "$SOLINSTALLCHECKSUM  install" | sha256sum -c - \
    && /bin/sh install

ENV PATH="$SOLPATH:$PATH"


RUN echo $(solana --version | awk '{print $2}') >> $SOLPATH/instsolversion

# Make dirs for build artefacts
RUN mkdir -p $SOLIDOBUILDPATH \
    && mkdir -p $SOLIDORELEASEPATH/deploy \
    && mkdir -p $SOLIDORELEASEPATH/cli

COPY . $SOLIDOBUILDPATH/

# Build packages
RUN cd $SOLIDOBUILDPATH \
    && cargo build-bpf

# Copy artefacts and remove build dirs
RUN cd $SOLIDOBUILDPATH \
    && cp -rf $SOLIDOBUILDPATH/target/deploy $SOLIDORELEASEPATH \
    && cp -rf $SOLIDOBUILDPATH/target/release/* $SOLIDORELEASEPATH/cli \
    && rm -rf $SOLIDOBUILDPATH



# Hash on-chain programs
RUN cd $SOLIDORELEASEPATH/deploy \
    && sha256sum lido.so >> lido.hash \
    && sha256sum multisig.so >> multisig.hash \
    && sha256sum spl_math.so >> spl_math.hash \
    && sha256sum spl_stake_pool.so >> spl_stake_pool.hash \
    && sha256sum spl_token.so >> spl_token.hash


# Hash CLI
RUN cd $SOLIDORELEASEPATH/cli \
    && sha256sum solido >> solido.hash


FROM debian:stable-slim@sha256:463cabea60abc361ab670570fe30549a6531cd9af4a1b8577b1c93e9b5a1d369

RUN apt -y update \
    && apt -y install curl clang lld libudev-dev libssl-dev pkg-config libhidapi-dev libjemalloc-dev

ENV SOLPATH="/root/.local/share/solana/install/active_release/bin"
ENV PATH="$SOLPATH:$PATH"

COPY --from=builder $SOLPATH $SOLPATH
COPY --from=builder /root/.cache/solana /root/.cache/solana

WORKDIR $SOLPATH

# Expose Solana ports for external access
EXPOSE 1024
EXPOSE 1027
EXPOSE 8899-9100
