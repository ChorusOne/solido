FROM debian:stable-slim

ENV SOLVERSION=1.6.8
ENV SOLINSTALLCHECKSUM=be0d60ba830b9f4910faa3d7095f7ff3666392d501af37720cf6d6d0704e6019
ENV SOLPATH="/root/.local/share/solana/install/active_release/bin"

RUN apt -y update \
    && apt -y install curl clang lld tar zstd \
    && curl -sSfLO https://release.solana.com/v$SOLVERSION/install \
    && echo "$SOLINSTALLCHECKSUM  install" | sha256sum -c -

ENV PATH="$SOLPATH:$PATH"

RUN sh install

RUN echo $(solana --version | awk '{print $2}') >> $SOLPATH/instsolversion

COPY ./build/target.tar.zst $SOLPATH/
COPY ./setup.sh $SOLPATH/


RUN zstd -d $SOLPATH/target.tar.zst \
    && tar -xvf $SOLPATH/target.tar
#    && rm $SOLPATH/target.tar.zst \
#   && rm $SOLPATH/target.tar

RUN /bin/sh $SOLPATH/setup.sh

WORKDIR $SOLPATH

EXPOSE 1024
EXPOSE 1027
EXPOSE 8899-9100





