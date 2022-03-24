#!/bin/bash

if [ $# -eq 0 ]
then
    exec /mnt/us/buck/buck-cli
else
    exec /mnt/us/buck/buck-cli $1
fi

return 0

