# Solido JS SDK

Typescript SDK to facilitate interacting with the Solido program in JS/TS applications

## Installation

### Yarn

```
$ yarn add @chorusone/solido.js
```

### npm

```
$ npm install @chorusone/solido.js
```

## Development

### Install packages
```
$ npm install
or
$ yarn
```

### Run TS watch script
```
$ npm run watch:ts
```

### Run tests watch script
```
$ npm run watch:test
```

### Build and Release
TODO


## Usage

### Javascript
```js
const SolidoJS = require("@chorusone/solido.js");
const snapshot = SolidoJS.getSnapshot(...params);
```

### ES6
```js
import SolidoJS from "@chorusone/solido.js";
const snapshot = SolidoJS.getSnapshot(...params);
```

## Examples

### Generate deposit instruction

```ts
const SolidoJS = require("@chorusone/solido.js");

const main = async () => {
  const depositInstruction = await SolidoJS.getDepositInstruction(
    wallet.address,
    wallet.stSolAddress,
    SolidoJS.MAINNET_PROGRAM_ADDRESSES,
    new Lamports(wallet.balanceInLamports)
  ); 
}

main();
```