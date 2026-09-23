import test from 'node:test';
import assert from 'node:assert/strict';
import {filterFrames,frameExcluded,frameStars,filename,subjectExcluded} from '../src/review.mjs';
test('manual zero stars wins over an automatic keeper, filtering preserves rejects',()=>{
 const frames=[{id:1,path:'C:\\Photos\\A.CR3',rating:.95,manual_stars:0,rejected:1},{id:2,path:'C:\\Photos\\B.CR3',rating:.85,manual_stars:null,rejected:0}];
 assert.equal(frameStars(frames[0]),0);assert.equal(frameStars(frames[1]),4);
 assert.deepEqual(filterFrames(frames,'',1,'all').map(f=>f.id),[2]);
 assert.deepEqual(filterFrames(frames,'a.cr3',0,'rejected').map(f=>f.id),[1]);
 assert.equal(filename(frames[0].path),'A.CR3');
});
test('manual ratings rescue automatic culls but never deliberate rejects',()=>{
 const frame={rejected:0,manual_stars:null};
 const automatic={rejected:0,bystander:0,cull_reason:'soft',stars:null};
 assert.equal(subjectExcluded(frame,automatic),true);
 assert.equal(subjectExcluded(frame,{...automatic,stars:3}),false);
 assert.equal(subjectExcluded(frame,{...automatic,rejected:1,stars:3}),true);
 assert.equal(frameExcluded(frame,[automatic]),true);
 assert.equal(frameExcluded(frame,[automatic,{...automatic,stars:3}]),false);
 assert.equal(frameExcluded({...frame,rejected:1},[{...automatic,stars:3}]),true);
});
