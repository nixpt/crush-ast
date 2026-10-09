# Last 2 lines of logs/app.log, then the number of ERROR lines across logs/*.log.
tail -n 2 logs/app.log
echo "errors: $(cat logs/*.log | grep -c ERROR)"
