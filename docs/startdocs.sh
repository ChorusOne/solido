#!/usr/bin/env bash

ARGERR="Please supply first argument to the script as either 'start' or 'stop'"

if which node > /dev/null
 then
   echo "node is installed, skipping..."
 else
  echo "Please install a recent version (>=14) of nodejs."
  exit 1
fi

npm install --save docusaurus
npm install --save remark-math@3
npm install --save rehype-katex

npx docusaurus start

