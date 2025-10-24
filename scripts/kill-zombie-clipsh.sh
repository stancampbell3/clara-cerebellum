#!/usr/bin/env bash
# find any process owned by this user named "clips" and sigterm it
pkill -f -u "$(whoami)" clips || true