import test from 'node:test';
import assert from 'node:assert/strict';
import {filterFrames,flawWords,frameExcluded,frameFlaw,frameStars,filename,subjectExcluded} from '../src/review.mjs';
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
test('a sharp focus area keeps a frame the detector found only bystanders in',()=>{
 const frame={rejected:0,manual_stars:null,af_sharpness:.90};
 const bystander={rejected:0,bystander:1,cull_reason:null,stars:null};
 // The detector found no vehicle and parked its person; the camera focused
 // sharp, so the frame is not culled.
 assert.equal(frameExcluded(frame,[bystander]),false);
 // Below the keeper bar the AF area is no rescue: the frame is culled as before.
 assert.equal(frameExcluded({...frame,af_sharpness:.50},[bystander]),true);
 // A deliberate reject still wins, whatever the focus area says.
 assert.equal(frameExcluded({...frame,rejected:1},[bystander]),true);
});

test('a frame with its own fault is excluded, unless a hand star or a bracket excuses it', () => {
  const blurred = { id: 1, rejected: 0, flaw: 'blurred', manual_stars: null, set_id: null };
  assert.equal(frameFlaw(blurred), 'blurred');
  assert.equal(frameExcluded(blurred, []), true);
  assert.equal(flawWords('blurred'), 'Nothing in this frame is in focus.');
  assert.equal(frameFlaw({ ...blurred, manual_stars: 4 }), null, 'a hand star outranks the fault');
  const blown = { id: 2, rejected: 0, flaw: 'overexposed', manual_stars: null, set_id: null };
  assert.equal(frameFlaw(blown), 'overexposed');
  assert.equal(frameFlaw({ ...blown, set_id: 3 }), null, 'a bracket member is not faulted for exposure');
  assert.equal(frameFlaw({ ...blurred, set_id: 3 }), 'blurred');
  assert.equal(frameFlaw({ id: 3, rejected: 0, flaw: null }), null);
});
