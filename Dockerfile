FROM rust:1.67

WORKDIR /usr/src/app

COPY . .

RUN cargo install --path .

RUN rm -r *

CMD ["isimud"]