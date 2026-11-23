import fs from 'node:fs/promises';
await fs.mkdir('dist',{recursive:true});await fs.cp('demo','dist',{recursive:true});console.log('Static demo built in dist/');
