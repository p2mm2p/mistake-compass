#!/bin/sh
cd "$(dirname "$0")"
./mistake-compass login
printf '\n按回车键关闭'
read -r _
