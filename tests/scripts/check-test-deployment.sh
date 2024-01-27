#!/bin/bash

# Polling kubectl to check pod status
end=$((SECONDS+60))  # 60 seconds from now
while [ $SECONDS -lt $end ]; do
    # Check if the pod is in Running status
    # app.kubernetes.io/name=navitain
    STATUS=$(kubectl get po -l app.kubernetes.io/name=navitain -o jsonpath="{.items[*].status.phase}")
    if [ "$STATUS" == "Running" ]; then
        echo "Pod is running"
        break
    fi
    echo "Waiting for pod to be in running state..."
    sleep 2  # polling interval
done

if [ "$STATUS" != "Running" ]; then
    echo "Timed out waiting for pod to become Running."
    exit 1
fi

