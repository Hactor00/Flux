import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
const [mode,binary,data,downloads,record] = process.argv.slice(2);
async function cli(args){return await new Promise((resolve,reject)=>{const child=spawn(join(binary,'flux-cli.exe'),['--data',data,'--output',downloads,...args],{windowsHide:true});let out='',error='';child.stdout.on('data',b=>out+=b);child.stderr.on('data',b=>error+=b);child.on('error',reject);child.on('exit',code=>code===0?resolve(out):reject(Error(error)));});}
if(mode==='create') {
 const payload=Buffer.alloc(4096,37);
 const server=createServer((req,res)=>{res.writeHead(200,{'Content-Type':'application/octet-stream','Content-Length':payload.length});res.end(payload);});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 try{await cli(['download',`http://127.0.0.1:${server.address().port}/sample.bin`,'keep-after-upgrade.bin']);}finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
 const settings=JSON.parse(await cli(['settings']));settings.theme='light';settings.locale='tr-TR';settings.connections=7;
 const file=join(data,'upgrade-settings.json');await writeFile(file,JSON.stringify(settings));await cli(['settings','--input',file]);
 const tasks=JSON.parse(await cli(['list']));assert.equal(tasks.length,1);assert.equal(tasks[0].status,'complete');
 await writeFile(record,JSON.stringify({tasks,settings,payload:payload.toString('hex')},null,2));
} else {
 const previous=JSON.parse(await readFile(record,'utf8'));
 const tasks=JSON.parse(await cli(['list']));const settings=JSON.parse(await cli(['settings']));
 assert.equal(tasks.length,1);assert.equal(tasks[0].id,previous.tasks[0].id);assert.equal(tasks[0].sha256,previous.tasks[0].sha256);assert.equal(tasks[0].destination,previous.tasks[0].destination);
 assert.equal((await readFile(tasks[0].destination)).toString('hex'),previous.payload);
 assert.equal(settings.theme,'light');assert.equal(settings.locale,'tr-TR');assert.equal(settings.connections,7);
 console.log('2.5.0 profile history, file identity and preferences preserved by 2.6.0.');
}
