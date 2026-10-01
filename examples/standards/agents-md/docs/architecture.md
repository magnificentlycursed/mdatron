# Architecture

## Module boundaries

The command-line tool in `src/` calls the API in `packages/api/` over HTTP and
never links against it.

## The API

Requests and responses are JSON; each endpoint's types live beside its handler.
