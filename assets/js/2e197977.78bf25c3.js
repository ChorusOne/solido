(self.webpackChunkdocs=self.webpackChunkdocs||[]).push([[923],{3905:function(e,t,o){"use strict";o.d(t,{Zo:function(){return u},kt:function(){return h}});var n=o(7294);function a(e,t,o){return t in e?Object.defineProperty(e,t,{value:o,enumerable:!0,configurable:!0,writable:!0}):e[t]=o,e}function i(e,t){var o=Object.keys(e);if(Object.getOwnPropertySymbols){var n=Object.getOwnPropertySymbols(e);t&&(n=n.filter((function(t){return Object.getOwnPropertyDescriptor(e,t).enumerable}))),o.push.apply(o,n)}return o}function l(e){for(var t=1;t<arguments.length;t++){var o=null!=arguments[t]?arguments[t]:{};t%2?i(Object(o),!0).forEach((function(t){a(e,t,o[t])})):Object.getOwnPropertyDescriptors?Object.defineProperties(e,Object.getOwnPropertyDescriptors(o)):i(Object(o)).forEach((function(t){Object.defineProperty(e,t,Object.getOwnPropertyDescriptor(o,t))}))}return e}function r(e,t){if(null==e)return{};var o,n,a=function(e,t){if(null==e)return{};var o,n,a={},i=Object.keys(e);for(n=0;n<i.length;n++)o=i[n],t.indexOf(o)>=0||(a[o]=e[o]);return a}(e,t);if(Object.getOwnPropertySymbols){var i=Object.getOwnPropertySymbols(e);for(n=0;n<i.length;n++)o=i[n],t.indexOf(o)>=0||Object.prototype.propertyIsEnumerable.call(e,o)&&(a[o]=e[o])}return a}var s=n.createContext({}),c=function(e){var t=n.useContext(s),o=t;return e&&(o="function"==typeof e?e(t):l(l({},t),e)),o},u=function(e){var t=c(e.components);return n.createElement(s.Provider,{value:t},e.children)},d={inlineCode:"code",wrapper:function(e){var t=e.children;return n.createElement(n.Fragment,{},t)}},g=n.forwardRef((function(e,t){var o=e.components,a=e.mdxType,i=e.originalType,s=e.parentName,u=r(e,["components","mdxType","originalType","parentName"]),g=c(o),h=a,p=g["".concat(s,".").concat(h)]||g[h]||d[h]||i;return o?n.createElement(p,l(l({ref:t},u),{},{components:o})):n.createElement(p,l({ref:t},u))}));function h(e,t){var o=arguments,a=t&&t.mdxType;if("string"==typeof e||a){var i=o.length,l=new Array(i);l[0]=g;var r={};for(var s in t)hasOwnProperty.call(t,s)&&(r[s]=t[s]);r.originalType=e,r.mdxType="string"==typeof e?e:a,l[1]=r;for(var c=2;c<i;c++)l[c]=o[c];return n.createElement.apply(null,l)}return n.createElement.apply(null,o)}g.displayName="MDXCreateElement"},3135:function(e,t,o){"use strict";o.r(t),o.d(t,{frontMatter:function(){return l},metadata:function(){return r},toc:function(){return s},default:function(){return u}});var n=o(2122),a=o(9756),i=(o(7294),o(3905)),l={title:"Solong",description:"Overview of user staking in LIDO for Solana with Solong",keywords:["staking","end-user","lido","solana","solong"],sidebar_label:"Solong",sidebar_position:5},r={unversionedId:"Guides/Staking/Wallets/Solong",id:"Guides/Staking/Wallets/Solong",isDocsHomePage:!1,title:"How to Stake Solana on Lido",description:"Overview of user staking in LIDO for Solana with Solong",source:"@site/docs/Guides/Staking/Wallets/Solong.md",sourceDirName:"Guides/Staking/Wallets",slug:"/Guides/Staking/Wallets/Solong",permalink:"/solido/docs/Guides/Staking/Wallets/Solong",version:"current",sidebar_label:"Solong",sidebarPosition:5,frontMatter:{title:"Solong",description:"Overview of user staking in LIDO for Solana with Solong",keywords:["staking","end-user","lido","solana","solong"],sidebar_label:"Solong",sidebar_position:5},sidebar:"solidoSidebar",previous:{title:"How to Stake Solana on Lido",permalink:"/solido/docs/Guides/Staking/Wallets/Solflare"},next:{title:"How to Stake Solana on Lido",permalink:"/solido/docs/Guides/Staking/Wallets/Ledger"}},s=[{value:"Introduction",id:"introduction",children:[]},{value:"Lido for Solana staking guide",id:"lido-for-solana-staking-guide",children:[]},{value:"Step 1: Create or Restore Solong Wallet",id:"step-1-create-or-restore-solong-wallet",children:[{value:"Creating the wallet",id:"creating-the-wallet",children:[]},{value:"Restoring the wallet",id:"restoring-the-wallet",children:[]},{value:"Logged In",id:"logged-in",children:[]}]},{value:"Step 2: Connect Lido to Solong",id:"step-2-connect-lido-to-solong",children:[]},{value:"Step 3: Explore the interface",id:"step-3-explore-the-interface",children:[{value:"Account info",id:"account-info",children:[]},{value:"Transaction Parameters",id:"transaction-parameters",children:[]},{value:"Lido Statistics",id:"lido-statistics",children:[]},{value:"FAQs",id:"faqs",children:[]}]},{value:"Step 4: Stake your SOL",id:"step-4-stake-your-sol",children:[]},{value:"Step 5: View the transaction on Blockexplorer",id:"step-5-view-the-transaction-on-blockexplorer",children:[]},{value:"Withdrawing Solana",id:"withdrawing-solana",children:[]},{value:"Resources",id:"resources",children:[]}],c={toc:s};function u(e){var t=e.components,l=(0,a.Z)(e,["components"]);return(0,i.kt)("wrapper",(0,n.Z)({},c,l,{components:t,mdxType:"MDXLayout"}),(0,i.kt)("p",null,"A quick guide on staking your Solana on the Lido widget"),(0,i.kt)("h2",{id:"introduction"},"Introduction"),(0,i.kt)("p",null,"\u2018Lido for Solana\u2019 is a Lido-DAO governed liquid staking protocol for the Solana blockchain. Anyone who stakes their SOL tokens with Lido will be issued an on-chain representation of SOL staking position with Lido validators, called stSOL. This will allow Solana token holders to get liquidity on their staked assets which can then be traded, or further utilized as collateral in DeFi products."),(0,i.kt)("p",null,(0,i.kt)("img",{alt:"Widget",src:o(923).Z})),(0,i.kt)("h2",{id:"lido-for-solana-staking-guide"},"Lido for Solana staking guide"),(0,i.kt)("p",null,"In this step-by-step guide, we will learn how to stake your Solana via the Lido staking widget. This guide shows the testnet for demonstration purposes. However, the process remains the same for mainnet. You can use one of the following wallets to connect to Lido. The facility to use the hardware wallet Ledger is also provided. This guarantees an extra layer of security for the user."),(0,i.kt)("ol",null,(0,i.kt)("li",{parentName:"ol"},"Sollet"),(0,i.kt)("li",{parentName:"ol"},"Phantom"),(0,i.kt)("li",{parentName:"ol"},"Solflare"),(0,i.kt)("li",{parentName:"ol"},"Solong"),(0,i.kt)("li",{parentName:"ol"},"Ledger")),(0,i.kt)("hr",null),(0,i.kt)("h2",{id:"step-1-create-or-restore-solong-wallet"},"Step 1: Create or Restore Solong Wallet"),(0,i.kt)("p",null,"Navigate to ",(0,i.kt)("a",{parentName:"p",href:"https://solongwallet.com/"},"https://solongwallet.com/")," to create/restore your solana wallet. You will need to install the ",(0,i.kt)("a",{parentName:"p",href:"https://chrome.google.com/webstore/detail/solong/memijejgibaodndkimcclfapfladdchj"},"browser extension")," offered by Solong to use this wallet."),(0,i.kt)("img",{src:"./images/solong/extension.png",alt:"Extension",width:"1000"}),(0,i.kt)("h3",{id:"creating-the-wallet"},"Creating the wallet"),(0,i.kt)("p",null,"If you do not have a wallet you yet, you should"),(0,i.kt)("ol",null,(0,i.kt)("li",{parentName:"ol"},"Create a new wallet,"),(0,i.kt)("li",{parentName:"ol"},"Note down your 12 word mnemonic, and"),(0,i.kt)("li",{parentName:"ol"},"your password, and\nstore these in a safe place. Follow the onscreen instructions and make sure to fund your wallet with some SOL tokens before interacting with Lido.")),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},"Note: Solong asks you to enter a password before creating or restoring a wallet.")),(0,i.kt)("img",{src:"./images/solong/create3.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("img",{src:"./images/solong/create1.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("img",{src:"./images/solong/create2.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("h3",{id:"restoring-the-wallet"},"Restoring the wallet"),(0,i.kt)("p",null,"If you already have a wallet, you can restore it through the Solong extension using the mnemonic. Follow the online instructions to restore your SOL account."),(0,i.kt)("img",{src:"./images/solong/create3.png",alt:"Restore wallet",width:"400",height:"600"}),(0,i.kt)("img",{src:"./images/solong/restore1.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("img",{src:"./images/solong/restore2.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("img",{src:"./images/solong/restore3.png",alt:"create wallet",width:"400",height:"600"}),(0,i.kt)("h3",{id:"logged-in"},"Logged In"),(0,i.kt)("p",null,"Once you have funded your Solong wallet with Solana tokens, you can log in to the extension to see your account details."),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/solong/logged_in.png",alt:"logged_in",width:"400"})),(0,i.kt)("h2",{id:"step-2-connect-lido-to-solong"},"Step 2: Connect Lido to Solong"),(0,i.kt)("p",null,"Once your wallet is setup visit ",(0,i.kt)("a",{parentName:"p",href:"https://solana.lido.fi"},"https://solana.lido.fi")," to stake your SOL tokens. Now connect your Solong account to the Lido interface."),(0,i.kt)("img",{src:"./images/common/connect.png",alt:"connect",width:"1000"}),(0,i.kt)("p",null,"Pressing the connect wallet button, on the top right hand corner of the screen, pops up the wallet screen."),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/solong/wallet_list.png",alt:"wallet_list",width:"400"})),(0,i.kt)("p",null,"Selecting your wallet and pressing the connect button takes you to another window with the wallet\u2019s browser extension. On this window you will have to ",(0,i.kt)("strong",{parentName:"p"},"approve the connection"),". Make sure to verify the details listed on the approval screen by Solong."),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/solong/approve_connection.png",alt:"Approve Connection",width:"400"})),(0,i.kt)("p",null,"If you have set a password to open the wallet, you might get prompted to unlock your wallet. You will, then, have to allow Lido to connect to your wallet and fetch its balance. Once connected you would be able to see your balance on the Lido widget."),(0,i.kt)("img",{src:"./images/solong/connected_widget.png",alt:"Connected Widget",width:"1000"}),(0,i.kt)("p",null,"Before you interact with the widget further it is important to explore the widget and understand its functionality."),(0,i.kt)("h2",{id:"step-3-explore-the-interface"},"Step 3: Explore the interface"),(0,i.kt)("p",null,"At the top you can see your account\u2019s information\u200a\u2014\u200ayour current stSOL balance and the number of SOL tokens available for staking. For new account holders, the staked amount (stSOL) will be 0 in the beginning. You can also see the returns you will get by staking with Lido under Lido APR. Below that you can enter the number of SOL you want to stake."),(0,i.kt)("img",{src:"./images/common/interface.png",alt:"Interface",width:"1000"}),(0,i.kt)("h3",{id:"account-info"},"Account info"),(0,i.kt)("p",null,"You can go to the top-right corner of the screen and click on your connected account. This lets you take a look at your address and disconnect at any point during the process.\nThe address for the demo account is"),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},(0,i.kt)("inlineCode",{parentName:"p"},"2HQHi4Lq9D9FHFFRBb7bSrvnyMPpPmvW5uC1b9N5K4Bg"))),(0,i.kt)("p",null,"Its transaction history can be viewed on the blockexplorer ",(0,i.kt)("a",{parentName:"p",href:"https://explorer.solana.com/address/2HQHi4Lq9D9FHFFRBb7bSrvnyMPpPmvW5uC1b9N5K4Bg?cluster=testnet"},"here"),"."),(0,i.kt)("img",{src:"./images/solong/connect_dialog.png",alt:"connect_dialog",width:"1000"}),(0,i.kt)("img",{src:"./images/solong/connect_dialog_2.png",alt:"connect_dialog",width:"1000"}),(0,i.kt)("h3",{id:"transaction-parameters"},"Transaction Parameters"),(0,i.kt)("p",null,"When you enter the amount of SOL you want to stake, the values below the submit button change automatically. These values give you specific information about the transaction you are about to perform. It tells you the"),(0,i.kt)("ul",null,(0,i.kt)("li",{parentName:"ul"},"Exchange rate of SOL v/s stSOL at the moment"),(0,i.kt)("li",{parentName:"ul"},"Amount of stSOL you will receive"),(0,i.kt)("li",{parentName:"ul"},"Transaction cost"),(0,i.kt)("li",{parentName:"ul"},"Fee that will be deducted for this transaction")),(0,i.kt)("img",{src:"./images/solong/tx_params.png",alt:"Transaction Parameters",width:"1000"}),(0,i.kt)("h3",{id:"lido-statistics"},"Lido Statistics"),(0,i.kt)("p",null,"Just below the transaction parameters you also see global Lido statistics. This gives you a clear idea of how much SOL is being staked worldwide and other information regarding the liquid staking ecosystem."),(0,i.kt)("img",{src:"./images/common/lido_params.png",alt:"Lido Parameters",width:"1000"}),(0,i.kt)("h3",{id:"faqs"},"FAQs"),(0,i.kt)("p",null,"You can see the FAQ section right below the Lido statistics. It is prudent to familiarize yourself with some of the basic features of liquid staking and the risks involved. The FAQ section also gives more information about the stSOL and its value. In case, you have even more questions you can always reach out to the Lido team or Chorus One for any clarifications. The contact information is given at the end of this article."),(0,i.kt)("img",{src:"./images/common/faqs.png",alt:"FAQs",width:"1000"}),(0,i.kt)("h2",{id:"step-4-stake-your-sol"},"Step 4: Stake your SOL"),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/common/stake.png",alt:"stake",width:"700"})),(0,i.kt)("p",null,"To stake your SOL with lido enter the amount you wanter to stake. Once you submit you might get redirected to your wallet. On the lido widget will see a pop-up showing the state of your transaction. The Lido widget waits for you to approve this transaction through your wallet."),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},"Note ",(0,i.kt)("strong",{parentName:"p"},"This transaction will only go through if you go back to your wallet and approve it."))),(0,i.kt)("img",{src:"./images/solong/staking.png",alt:"staking",width:"1000"}),(0,i.kt)("p",null,"You get additional information about the transaction details while approving the transaction. Go ahead and approve the transaction."),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/solong/approve.png",alt:"Approve Transaction",width:"400"})),(0,i.kt)("p",null,"After verifying the information you can approve now."),(0,i.kt)("h2",{id:"step-5-view-the-transaction-on-blockexplorer"},"Step 5: View the transaction on Blockexplorer"),(0,i.kt)("p",null,"Once you hit approve on your wallet, you can come back to the lido widget and click on ",(0,i.kt)("strong",{parentName:"p"},"View on Solana Blockexplorer.")),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/common/view_tx.png",alt:"view_tx",width:"500"})),(0,i.kt)("p",null,"This is useful as it tells you the ",(0,i.kt)("a",{parentName:"p",href:"https://explorer.solana.com/tx/czfRH3ZZbvuU6BEBizVV2CmSgg4JQ4bR1zwz4T4H9xu8fvMPWiREmDgMDm4bgCHkjSq56Jy1FXiTe1kydoojsyc?cluster=testnet"},"current status")," of your transaction."),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},"Link for the above transaction - ",(0,i.kt)("a",{parentName:"p",href:"https://explorer.solana.com/tx/czfRH3ZZbvuU6BEBizVV2CmSgg4JQ4bR1zwz4T4H9xu8fvMPWiREmDgMDm4bgCHkjSq56Jy1FXiTe1kydoojsyc?cluster=testnet"},"https://explorer.solana.com/tx/czfRH3ZZbvuU6BEBizVV2CmSgg4JQ4bR1zwz4T4H9xu8fvMPWiREmDgMDm4bgCHkjSq56Jy1FXiTe1kydoojsyc?cluster=testnet"))),(0,i.kt)("p",null,"If you look at the Confirmations field you can slowly see it increasing from 0 to 32. Once it reaches the MAX number of confirmations your transaction gets added to the blockchain"),(0,i.kt)("img",{src:"./images/solong/confirmations1.png",alt:"View on Blockexplorer",width:"1000"}),(0,i.kt)("img",{src:"./images/solong/confirmations2.png",alt:"View on Blockexplorer",width:"1000"}),(0,i.kt)("img",{src:"./images/solong/confirmations3.png",alt:"View on Blockexplorer",width:"1000"}),(0,i.kt)("p",null,"Finally, after 32 confirmations the transaction gets confirmed. The lido widget will reflect the new balance"),(0,i.kt)("img",{src:"./images/solong/confirmations4.png",alt:"View on Blockexplorer",width:"1000"}),(0,i.kt)("p",null,"You can now go back to the Lido widget and look at your updated stSOL balance. Just below the stSOL balance, you will also be able to see the amount of SOL you can get back for it a.k.a the exchange rate."),(0,i.kt)("img",{src:"./images/solong/update.png",alt:"update",width:"1000"}),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/solong/updated.png",alt:"updated",width:"400"})),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},"Note 1: We had 2 SOL in the beginning and we staked 1 SOL. We should be left with 1 SOL but we had to pay an additional 0.0021 SOL as the rent for the new stSOL account that got created for us.")),(0,i.kt)("blockquote",null,(0,i.kt)("p",{parentName:"blockquote"},"Note 2: This rent is a one-time fee that won\u2019t reccur on the next staking transaction.")),(0,i.kt)("h2",{id:"withdrawing-solana"},"Withdrawing Solana"),(0,i.kt)("p",null,"Withdrawals are not enabled yet. They will be live within the coming months. If you click on the ",(0,i.kt)("strong",{parentName:"p"},"Unstake")," tab you will see an error message pop up."),(0,i.kt)("p",{align:"center"},(0,i.kt)("img",{src:"./images/common/unstake.png",alt:"Unstake",width:"450"})),(0,i.kt)("h2",{id:"resources"},"Resources"),(0,i.kt)("p",null,(0,i.kt)("a",{parentName:"p",href:"https://medium.com/chorus-one/introducing-lido-for-solana-8aa02db8503"},"Introducing Lido for Solana")," - Explaining the SOL liquid staking solution by Chorus One"))}u.isMDXComponent=!0},923:function(e,t,o){"use strict";t.Z=o.p+"assets/images/widget-e2704ae984009eee59cfd911dc01736a.png"}}]);