FROM rust:1.67 as builder
WORKDIR /usr/src/book-archive-service
COPY . .
RUN cargo install --path .

FROM debian:bullseye-slim
#No extra dependencies I think?
#RUN apt-get update && apt-get install -y extra-runtime-dependencies && rm -rf /var/lib/apt/lists/* 
COPY --from=builder /usr/local/cargo/bin/book-archive-service /usr/local/bin/book-archive-service
CMD ["book-archive-service"]
