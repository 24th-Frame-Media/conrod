import test from 'node:test';
import assert from 'node:assert/strict';
import {filterFrames,frameStars,filename} from '../src/review.mjs';
test('manual zero stars wins over an automatic keeper, filtering preserves rejects',()=>{
 const frames=[{id:1,path:'C:\\Photos\\A.CR3',rating:.95,manual_stars:0,rejected:1},{id:2,path:'C:\\Photos\\B.CR3',rating:.85,manual_stars:null,rejected:0}];
 assert.equal(frameStars(frames[0]),0);assert.equal(frameStars(frames[1]),4);
 assert.deepEqual(filterFrames(frames,'',1,'all').map(f=>f.id),[2]);
 assert.deepEqual(filterFrames(frames,'a.cr3',0,'rejected').map(f=>f.id),[1]);
 assert.equal(filename(frames[0].path),'A.CR3');
});
