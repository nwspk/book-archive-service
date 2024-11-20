FROM docker.io/alpine:3.20

WORKDIR /app

## copy the main binary
COPY  ./cross-compiled/book-archive-service ./

## copy runtime assets which may or may not exist
COPY  ./stati[c] ./static
COPY  ./template[s] ./templates

## ensure the container listens globally on port 8080g
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8090

CMD ./book-archive-service
