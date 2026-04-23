#!/usr/bin/env bash
set -euo pipefail

CLI="./target/release/syncenv-cli"

echo "==> Building and starting nodes..."
docker compose up -d --build

echo "==> Waiting for daemons to be ready..."
sleep 3

echo ""
echo "==> [node1] Creating document..."
TICKET=$(docker compose exec -T node1 $CLI join-or-create)
echo "Ticket: $TICKET"

echo ""
echo "==> [node2] Joining document..."
docker compose exec -T node2 $CLI join-or-create --ticket "$TICKET"

echo ""
echo "==> [node1] Setting FOO=bar in profile 'test'..."
docker compose exec -T node1 $CLI set-env --profile test --key FOO --val bar

echo ""
echo "==> Waiting for sync..."
sleep 5

echo ""
echo "==> [node2] Getting FOO from profile 'test'..."
VALUE=$(docker compose exec -T node2 $CLI get-env --profile test --key FOO)
echo "FOO=$VALUE"

if [ "$VALUE" = "bar" ]; then
    echo ""
    echo "==> SUCCESS: value synced correctly."
else
    echo ""
    echo "==> FAIL: expected 'bar', got '$VALUE'."
    exit 1
fi

echo ""
echo "==> To clean up: docker compose down -v"
