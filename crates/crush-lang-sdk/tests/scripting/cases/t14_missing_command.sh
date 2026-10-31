# Run a command that doesn't exist; report it without dying.
not-a-command-crush-242 2>/dev/null || echo "missing"
