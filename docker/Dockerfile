FROM osgeo/gdal:ubuntu-small-3.2.3
RUN apt-get update && apt-get install -y \
    build-essential curl clang cmake pkg-config && apt-get update

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

COPY --from=georust/libproj-builder:rust-1.51 /build/usr /usr

WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .

CMD ["autostac"]
