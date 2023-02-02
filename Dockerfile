FROM rust:1.67-slim-buster

WORKDIR /usr/src/app

COPY . .

RUN cargo install --path .

RUN rm -r *

ENTRYPOINT [ "isimud" ]