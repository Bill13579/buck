#!/bin/bash

cd $(dirname "$0")

if [ $# -eq 0 ]
then
    exec ./buck-cli
else
    exec ./buck-cli $1
fi

return 0

