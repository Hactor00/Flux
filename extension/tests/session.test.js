import test from "node:test";
import assert from "node:assert/strict";
import { refreshCredentials } from "../session.js";

test("fresh session cookies stay scoped to the requested URL and container", async()=>{
  const calls=[];
  const api={runtime:{},tabs:{get:async()=>({url:"https://watch.test/film",cookieStoreId:"firefox-container-2"})},
    cookies:{getAll:async(details)=>{calls.push(details);return [
      {name:"session",value:"new",path:"/media",domain:"cdn.test"},
      {name:"bad\r\n",value:"header-injection"},
      {name:"expired",value:"old",expirationDate:1},
    ];}}};
  const saved=[{origin:"https://cdn.test",cookie:"session=old",authorization:null,user_agent:null},
    {origin:"https://unrelated.test",cookie:"private"}];
  const result=await refreshCredentials(api,7,"https://cdn.test/media/film.mp4",saved);
  assert.deepEqual(calls,[{url:"https://cdn.test/media/film.mp4",storeId:"firefox-container-2"}]);
  assert.equal(result.length,1);
  assert.equal(result[0].cookie,"session=new");
  assert.equal(saved[0].cookie,"session=old");
});

test("partitioned cookies use the exact active frame partition", async()=>{
  const calls=[];
  const key={topLevelSite:"https://watch.test",hasCrossSiteAncestor:true};
  const api={runtime:{},tabs:{get:async()=>({url:"https://watch.test/film"})},cookies:{
    getPartitionKey:async(details)=>{assert.deepEqual(details,{tabId:4,frameId:2});return {partitionKey:key};},
    getAll:async(details)=>{calls.push(details);return details.partitionKey ? [{name:"partitioned",value:"right",partitionKey:key}] :
      [{name:"regular",value:"yes"},{name:"wrong",value:"no",partitionKey:{topLevelSite:"https://other.test"}}];}}};
  const result=await refreshCredentials(api,4,"https://cdn.test/file",[],[],2);
  assert.equal(result[0].cookie,"regular=yes; partitioned=right");
  assert.deepEqual(calls[1].partitionKey,key);
});

test("Firefox native consent prevents cookie reads and container failures retain recent scoped headers",async()=>{
  let reads=0;
  const blocked={runtime:{getBrowserInfo(){}},storage:{local:{get:async()=>({nativeConsent:false})}},cookies:{getAll:async()=>{reads++;return [];}}};
  assert.deepEqual(await refreshCredentials(blocked,1,"https://cdn.test/file",[]),[]);
  assert.equal(reads,0);
  const fallback={runtime:{},tabs:{get:async()=>{throw Error("Closed tab");}},cookies:{getAll:async()=>{throw Error("Unavailable store");}}};
  const result=await refreshCredentials(fallback,1,"https://cdn.test/file",[{origin:"https://cdn.test",cookie:"session=recent"}]);
  assert.equal(result[0].cookie,"session=recent");
});
