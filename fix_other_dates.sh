#!/bin/bash
TODAY=$(date +%Y-%m-%d)
sed -i "s/2025-03-05/$TODAY/g" .jules/sentinel.md
