'use strict';
const $=s=>document.querySelector(s);
const token=$('meta[name="session-token"]').content;
let state={connected:false,selection:[]},settings={presets:{}},busy=false,previewUrl=null;
const forms=['grid-form','labels-form','borders-form'];
function message(text,type=''){const el=$('#message');el.textContent=text;el.className='message '+type;}
async function api(path,payload){
  const options={headers:{'X-Toolbox-Token':token}};
  if(payload!==undefined){options.method='POST';options.headers['Content-Type']='application/json';options.body=JSON.stringify(payload);}
  const response=await fetch(path,options);
  const data=await response.json();
  if(!response.ok)throw new Error(data.error||'请求未成功');
  return data;
}
function formData(id){const f=$('#'+id),result=Object.fromEntries(new FormData(f));
  for(const input of f.querySelectorAll('input[type=checkbox]'))result[input.name]=input.checked;
  return result;}
function fillForm(id,data){for(const [name,value]of Object.entries(data||{})){const el=$('#'+id).elements.namedItem(name);if(!el)continue;if(el.type==='checkbox')el.checked=!!value;else el.value=value;}}
function fmt(n){return Number(n).toFixed(1).replace(/\.0$/,'');}
function showState(next){
  state=next;$('#lamp').classList.toggle('on',!!state.connected);
  $('#connection-label').textContent=state.connected?'桌面窗口已连接':'尚未连接桌面窗口';
  $('#document-title').textContent=state.document||(state.connected?'请在专用窗口打开画稿':'尚未连接');
  $('#selection-count').textContent=state.selection?.length||0;
  $('#revision').textContent=state.revision===undefined||state.revision===null?'':'修订 '+state.revision;
  const list=$('#selection-list');list.replaceChildren();
  if(!state.selection?.length){const p=document.createElement('p');p.className='empty';p.textContent=state.connected?'在 VectorCraft 中选中图片或对象，再点击刷新选择。':'连接后，在 VectorCraft 中选择图片或对象。';list.append(p);}
  for(const object of state.selection||[]){const row=document.createElement('div');row.className='object-row';
    const icon=document.createElement('span');icon.className='object-icon';icon.textContent=object.kind==='Text'?'Aa':object.kind==='Group'?'▦':'□';
    const body=document.createElement('div');body.className='object-body';const name=document.createElement('strong');name.textContent=object.name||object.kind||'对象';
    const details=document.createElement('small');const b=object.bounds;details.textContent=b?`${fmt(b.width)} × ${fmt(b.height)} pt  ·  x ${fmt(b.x)}, y ${fmt(b.y)}`:'没有可用边界';
    const id=document.createElement('span');id.className='object-id';id.textContent='#'+object.id;
    body.append(name,details);row.append(icon,body,id);list.append(row);}
  $('#clipboard-state').textContent=state.clipboard?'已记录位置；选择对应的目标对象后粘贴。':'尚未记录位置';
  $('#uncertain-panel').hidden=!state.uncertain;
  if(!state.connected)$('#connection-panel').hidden=false;
  updateDisabled();
}
function updateDisabled(){for(const b of document.querySelectorAll('button')){
  const always=b.closest('nav')||['show-connection','hide-connection','confirm-preset'].includes(b.id)||b.closest('#preset-dialog');
  if(always){b.disabled=false;continue;}
  b.disabled=busy;
  if(b.matches('[data-op],[data-align],[data-spacing]')||b.closest('.tool-section')||b.id==='resize-only')b.disabled=busy||!state.connected||!state.document||state.uncertain;
  if(b.id==='paste-position')b.disabled=b.disabled||!state.clipboard;
  if(b.id==='preview-button')b.disabled=busy||!state.document;
}}
function renderPresets(){const list=$('#preset-list'),saved=list.value;list.replaceChildren(new Option('选择预设',''));for(const name of Object.keys(settings.presets||{}))list.add(new Option(name,name));list.value=saved;}
async function refresh(updateSettings=false){const data=await api('/api/state');if(updateSettings){settings=data.settings;$('#executable').value=settings.executable;$('#root').value=settings.root;$('#control-port').value=data.port;$('#version').textContent=data.version;renderPresets();}showState(data.state);return data;}
async function task(fn){if(busy)return;busy=true;document.body.classList.add('busy');updateDisabled();try{await fn();}catch(e){message(e.message,'error');try{await refresh();}catch{}}finally{busy=false;document.body.classList.remove('busy');updateDisabled();}}
async function action(operation,options={}){await task(async()=>{message('正在处理，请暂停手动编辑…');const result=await api('/api/action',{operation,options,revision:state.revision,documentIndex:state.documentIndex,selection:state.selection.map(o=>o.id),title:state.document});showState(result.state);message(result.message,'success');});}
for(const b of document.querySelectorAll('.nav'))b.addEventListener('click',()=>{for(const n of document.querySelectorAll('.nav'))n.classList.toggle('active',n===b);for(const section of document.querySelectorAll('[data-section]'))section.hidden=section.dataset.section!==b.dataset.panel;});
$('#show-connection').onclick=()=>$('#connection-panel').hidden=!$('#connection-panel').hidden;
$('#hide-connection').onclick=()=>$('#connection-panel').hidden=true;
$('#refresh').onclick=()=>task(async()=>{await refresh();message('选择与连接状态已更新');});
$('#connect').onclick=()=>task(async()=>{const result=await api('/api/connect',{port:$('#control-port').value});showState(result.state);message(result.message,'success');});
$('#launch').onclick=()=>task(async()=>{message('正在启动专用窗口…');const result=await api('/api/launch',{port:$('#control-port').value,root:$('#root').value,executable:$('#executable').value});settings=result.settings;showState(result.state);message(result.message,'success');});
$('#save-settings').onclick=()=>task(async()=>{const result=await api('/api/settings',{root:$('#root').value,executable:$('#executable').value});message(result.message,'success');});
$('#grid-form').onsubmit=e=>{e.preventDefault();action('grid',formData('grid-form'));};
$('#resize-only').onclick=()=>{if($('#grid-form').reportValidity())action('resize',formData('grid-form'));};
$('#labels-form').onsubmit=e=>{e.preventDefault();action('labels',formData('labels-form'));};
$('#borders-form').onsubmit=e=>{e.preventDefault();action('borders',formData('borders-form'));};
for(const b of document.querySelectorAll('[data-op]'))b.onclick=()=>action(b.dataset.op);
for(const b of document.querySelectorAll('[data-align]'))b.onclick=()=>action('align',{axis:b.dataset.align,anchor:$('#align-anchor').value});
for(const b of document.querySelectorAll('[data-spacing]'))b.onclick=()=>action('spacing',{axis:b.dataset.spacing,gap:$('#spacing').value});
$('#copy-position').onclick=()=>action('copy_position',{mode:$('#position-mode').value,order:$('#position-order').value});
$('#paste-position').onclick=()=>action('paste_position',{order:$('#position-order').value});
$('#acknowledge').onclick=()=>action('acknowledge');
$('#preview-button').onclick=()=>task(async()=>{message('正在读取实际画板预览…');const r=await fetch('/api/preview',{headers:{'X-Toolbox-Token':token}});if(!r.ok){const e=await r.json();throw new Error(e.error);}const blob=await r.blob();if(previewUrl)URL.revokeObjectURL(previewUrl);previewUrl=URL.createObjectURL(blob);$('#preview-image').src=previewUrl;$('#preview-image').hidden=false;$('#preview-empty').hidden=true;message('实际画板预览已更新');});
$('#save-preset').onclick=()=>{$('#preset-name').value='';$('#preset-dialog').showModal();};
$('#preset-dialog').addEventListener('close',()=>{if($('#preset-dialog').returnValue!=='save')return;const name=$('#preset-name').value.trim();if(!name)return;task(async()=>{const presets={...settings.presets,[name]:Object.fromEntries(forms.map(id=>[id,formData(id)]))};await api('/api/settings',{presets});settings.presets=presets;renderPresets();$('#preset-list').value=name;message('预设已保存','success');});});
$('#load-preset').onclick=()=>{const preset=settings.presets?.[$('#preset-list').value];if(!preset)return message('请先选择预设','error');for(const id of forms)fillForm(id,preset[id]);message('预设参数已载入；尚未修改画稿');};
$('#delete-preset').onclick=()=>task(async()=>{const name=$('#preset-list').value;if(!name)throw new Error('请先选择预设');const presets={...settings.presets};delete presets[name];await api('/api/settings',{presets});settings.presets=presets;renderPresets();message('预设已删除');});
$('#shutdown').onclick=()=>task(async()=>{const result=await api('/api/shutdown',{});showState({connected:false,selection:[]});message(result.message+'。可以关闭此页面。','success');});
refresh(true).then(()=>message(state.connected?'已连接桌面窗口。确认选择后即可排版。':'就绪。请在连接设置中启动专用窗口。')).catch(e=>message('无法读取工具箱：'+e.message,'error'));
