# Append a line to the log, then show its last two lines.
printf 'rotation: done\n' >> logs/app.log
tail -n 2 logs/app.log
