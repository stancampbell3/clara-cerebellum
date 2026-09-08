#!/usr/bin/bash
# fierypit.sh — front script for a standalone remote FieryPit deployment
# (docker-compose.remote-fierypit.yml), the "./clara" of a host that only
# lends a FieryPit to someone else's Dis domain rather than running the
# full local stack. See docs/remote_fierypit_deployment.md.
#
# Deliberately does NOT share a name with clara.sh/./clara: those front
# docker-compose.yml (the full local stack: kafka+clara-api+lildaemon+...)
# and clara-cerebellum/docker/.env. Both compose files default to the
# same Compose project name ("docker", from the containing directory) and
# the same service name ("lildaemon"), so running ./clara on a
# remote-fierypit host doesn't fail cleanly -- it silently creates or
# replaces a same-named container wired up via the WRONG compose file and
# a full-stack .env full of placeholder secrets, completely disconnected
# from the real Dis domain this host is supposed to join. Confirmed live
# 2026-09-08 on pineal — see remote_fierypit_deployment.md's "Real bugs
# found" section for the full incident. Use THIS script on a
# remote-fierypit host instead, never ./clara.
#
# Install on a new remote-fierypit host, from its Development/ directory
# (sibling to the lildaemon/clara-cerebellum checkouts):
#   ln -sf clara-cerebellum/scripts/fierypit.sh fierypit.sh
#   ln -sf ./fierypit.sh fierypit
# Then use exactly like ./clara: ./fierypit up -d, ./fierypit ps,
# ./fierypit logs -f, ./fierypit down, etc.
docker compose -f clara-cerebellum/docker/docker-compose.remote-fierypit.yml --env-file clara-cerebellum/docker/remote-fierypit.env $@
