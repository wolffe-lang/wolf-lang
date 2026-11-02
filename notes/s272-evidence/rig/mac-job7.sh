#!/bin/bash
# s272 mac job 7: jobs 5 and 6 in one hold of the Mac lock (run by build-at.sh).
/Users/mfwolffe/lanes/s272/mac-job5.sh
a=$?
/Users/mfwolffe/lanes/s272/mac-job6.sh
b=$?
echo "JOB7 job5=$a job6=$b"
[ $a -eq 0 ] && [ $b -eq 0 ]
