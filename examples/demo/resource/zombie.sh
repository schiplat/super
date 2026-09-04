cat > zombie.sh <<EOF
#!/bin/bash
# Trap SIGTERM (15) and log a message without exiting
trap "echo 'Haha! I am ignoring SIGTERM'" SIGTERM

echo "Zombie process started. I will live forever!"
while true; do
    sleep 1
done
EOF
