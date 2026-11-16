// SPDX-License-Identifier: AGPL-3.0-or-later
// A source preview uses published engine assets and the actual result query server.
import assert from 'node:assert/strict';
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {createServer} from 'node:http';
import {appSource} from './app_source.mjs';
import {openApp} from './cdp.mjs';
const backend=process.argv[2],{target,scene,shown,libraryTarget=target}=JSON.parse(process.argv[3]);
assert(['127.0.0.1','localhost'].includes(new URL(backend).hostname));
const visible={shown};let app;
const site=resolve(new URL('../site/',import.meta.url).pathname),web=resolve(new URL('../web/src/static/',import.meta.url).pathname);
let previewSource;
{
 const index=readFileSync(join(site,'index.html'),'utf8'),appPath=index.match(/src="(\/asset\/app\.[^"]+\.js)"/)[1];
 const built=readFileSync(join(site,appPath),'utf8');previewSource=appSource();
 const worker=built.match(/new Worker\("([^"]+)"\)/)[1];
 previewSource=previewSource.replace('new Worker("/worker.js")',`new Worker(${JSON.stringify(worker)})`);
 for(const key of ['ENGINE_WASM','ENGINE_WASM_BYTES','ENGINE_PACKS','ENGINE_VIEWS','PAGE_BODIES_URL']){
  const declaration=new RegExp('const '+key+' = [^\\n]*;');
  assert(declaration.test(built)&&declaration.test(previewSource),key+' must retain the published engine binding');
  previewSource=previewSource.replace(declaration,built.match(declaration)[0]);
 }
}
function previewAsset(path,res){
 const mime={'.js':'text/javascript','.css':'text/css','.json':'application/json','.wasm':'application/wasm','.svg':'image/svg+xml','.png':'image/png','.webp':'image/webp'};
 if(path==='/app.js'){res.writeHead(200,{'content-type':'text/javascript'});res.end(previewSource);return;}
 for(const directory of [web,site]){
  const file=resolve(directory,'.'+path);
  if(!file.startsWith(directory+'/'))continue;
  try{const body=readFileSync(file),extension=file.slice(file.lastIndexOf('.'));
   res.writeHead(200,{'content-type':mime[extension]||'application/octet-stream'});res.end(body);return;
  }catch{/* another asset root */}
 }
 if(path.startsWith('/api/')||path.includes('.')){res.writeHead(404);res.end();return;}
 const html=readFileSync(join(web,'index.html'),'utf8').replace('<script src="/app.js">','<script>window.WFSIM_WASM=true;</script><script src="/app.js">');
 res.writeHead(200,{'content-type':'text/html'});res.end(html);
}
const server=createServer(async(req,res)=>{
 try{
  const path=new URL(req.url,'http://127.0.0.1').pathname;
  if(path.startsWith('/api/board/pages')||path.startsWith('/api/recommendations/')){
   const response=await fetch(backend+req.url);
   res.writeHead(response.status,Object.fromEntries(response.headers));res.end(Buffer.from(await response.arrayBuffer()));return;
  }
  previewAsset(path,res);
 }catch(error){res.writeHead(500);res.end(String(error));}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base='http://127.0.0.1:'+server.address().port;
try{
  app=await openApp({base,lang:'en'});
  await app.load('/weapons/'+libraryTarget.weapon+'/build-library?'+new URLSearchParams({bench:libraryTarget.ruler,mode:libraryTarget.mode,riven:'0'}));
  const display=await app.evaluate(`(async()=>{
    await loadBoard(${JSON.stringify(target.weapon)});renderLibraryBoard();renderBuildFinder();
    const item=builtinBuilds(${JSON.stringify(libraryTarget.ruler)}).find(p=>p.board.score===${libraryTarget.score}&&(p.mode||'base')===${JSON.stringify(libraryTarget.mode)});
    const row=item&&[...document.querySelectorAll('[data-frow]')].find(row=>row.querySelector('.fd-sv b')?.textContent===item.board.shown);
    row?.scrollIntoView({block:'center'});
    return {found:!!item,text:row?.textContent||'',shown:item?.board.shown,weapon:finder.weapon,ruler:finder.b,path:location.pathname,active:libraryActive};
  })()`);
  assert.equal(display.weapon,libraryTarget.weapon,JSON.stringify(display));assert.equal(display.ruler,libraryTarget.ruler,JSON.stringify(display));
  assert(display.found&&display.shown&&display.text.includes(display.shown),'actual build-library DOM must show the accepted score: '+JSON.stringify(display));
  const output=resolve('target/board-result-flow');mkdirSync(output,{recursive:true});
  const image=await app.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  writeFileSync(join(output,'accepted-result.png'),Buffer.from(image.result.data,'base64'));
  await app.load('/benchmark?'+new URLSearchParams({ruler:target.ruler,measured:'1',riven:'plain'}));
  const board=await app.evaluate(`(async()=>{
    ${scene?`await loadSceneIndex(${JSON.stringify(scene)});`:'await loadFullBoard();'}
    benchPick=${JSON.stringify(target.ruler)};const weapon=META.weapons.find(w=>w.id===${JSON.stringify(target.weapon)});benchQuery=weapon.name_en||weapon.name||weapon.id;renderBenchBoard();
    return {text:document.getElementById('bench-board').textContent,query:benchQuery};
  })()`);
  assert(board.text.includes(visible.shown),'actual cross-weapon board DOM must show the accepted score: '+JSON.stringify(board));
  console.log(JSON.stringify({status:'ACTUAL_BROWSER_DOM_PASSED',build_library:display,board:board.query,screenshot:join(output,'accepted-result.png')}));
}finally{await app?.close();await new Promise(resolve=>server.close(resolve));}
