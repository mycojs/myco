#!/bin/bash
if [ $# -eq 0 ]; then
    echo "Hello from script!"
else
    echo "Script args: $*"
fi
