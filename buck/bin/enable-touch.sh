# BASED ON TWOBOB's go-game.sh and stop-games.sh scripts
# https://www.mobileread.com/forums/showthread.php?t=194270

export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:/mnt/us/buck/lib

#!/bin/sh

for i in $( /mnt/us/buck/bin/xdotool search --classname webreader ); do
    echo item: $i
    /mnt/us/buck/bin/xdotool windowactivate $i
done

#xdotool windowminimize 0x1200002  ==
for i in $(xwininfo -tree -root | grep "home" | \
    grep -o -e 0x[a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9]* ); do
    echo item: $i
    /mnt/us/buck/bin/xdotool windowactivate $i
done

for i in $(xwininfo -tree -root | grep "blankBackground" | \
    grep -o -e 0x[a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9]* ); do
    echo item: $i
    /mnt/us/buck/bin/xdotool windowactivate $i
done

for i in $(xwininfo -tree -root | grep "searchBar" | \
    grep -o -e 0x[a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9][a-z0-9]* ); do
    echo item: $i
    /mnt/us/buck/bin/xdotool windowactivate $i
done

/mnt/us/buck/bin/wmctrl -r L:A_N:titleBar_ID:system -e '0,0,0,600,30'