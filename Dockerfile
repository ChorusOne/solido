FROM rust:slim-buster@sha256:bbf94ba964b3d9a47600d8f5e2275fc461df300300216dc9f173e42e46300f74

ENV SOLVERSION=1.7.3
ENV SOLINSTALLCHECKSUM=b5ef6183da7d489a0e6ebc203d64e82e4dd24a9144ead3e564df692625d91b10
ENV SOLPATH="/root/.local/share/solana/install/active_release/bin"
ENV SOLIDOBUILDPATH="$SOLPATH/solido-build"
ENV SOLIDORELEASEPATH="$SOLPATH/solido"

# Install Solana tools
RUN apt -y update \
    && apt -y install libssl-dev libudev-dev pkg-config zlib1g-dev llvm clang make curl python3 \
    && curl -sSfLO https://release.solana.com/v$SOLVERSION/install \
    && echo "$SOLINSTALLCHECKSUM  install" | sha256sum -c - \
    && /bin/sh install

ENV PATH="$SOLPATH:$PATH"


RUN echo $(solana --version | awk '{print $2}') >> $SOLPATH/instsolversion

# Make dirs for build artefacts
RUN mkdir -p $SOLIDOBUILDPATH \
    && mkdir -p $SOLIDORELEASEPATH/deploy \
    && mkdir -p $SOLIDORELEASEPATH/cli \
    && mkdir -p $SOLIDORELEASEPATH/tests

COPY . $SOLIDOBUILDPATH/

# Build packages
RUN cd $SOLIDOBUILDPATH \
    && cargo build-bpf \
    && cargo build --release

# Copy artefacts and remove build dirs
RUN cd $SOLIDOBUILDPATH \
    && cp -rf $SOLIDOBUILDPATH/target/deploy $SOLIDORELEASEPATH \
    && cp -rf $SOLIDOBUILDPATH/target/release/* $SOLIDORELEASEPATH/cli \
    && cp -rf $SOLIDOBUILDPATH/tests/* $SOLIDORELEASEPATH/tests \
    && rm -rf $SOLIDOBUILDPATH

# Hash on-chain programs
RUN cd $SOLIDORELEASEPATH/deploy \
    && sha256sum lido.so >> lido.hash \
    && sha256sum multisig.so >> multisig.hash \


# Hash CLI
RUN cd $SOLIDORELEASEPATH/cli \
    && sha256sum solido >> solido.hash

WORKDIR $SOLPATH

# Expose Solana ports for external access
EXPOSE 1024
EXPOSE 1027
EXPOSE 8899-9100
