type Release = {tag:string;name:string;notes:string;url:string;prerelease:boolean;asset:{name:string;bytes:number}};
type Target = {id:string;name:string;repository:string;publisher_configured:boolean;installed:{version:string;build:string;tag:string}|null};
type Job = {id:string;target:string;phase:string;message:string;finished:boolean;release:{tag:string;version:string;build:string}|null};
type Status = {managed:boolean;version:string;status_port?:number;targets:Target[];job:Job|null};
type Hooks = {api:<T>(path:string,options?:RequestInit)=>Promise<T>;unsaved:()=>boolean;confirm:(title:string,description:string,accept:string)=>Promise<boolean>;changed:()=>void;retain:(build:()=>void)=>void};
const el=<K extends keyof HTMLElementTagNameMap>(tag:K,content='',className='')=>{const n=document.createElement(tag);n.textContent=content;n.className=className;return n;};
const button=(text:string,action:()=>void,disabled=false)=>{const b=el('button',text);b.type='button';b.onclick=action;b.disabled=disabled;return b;};
export class UpdatePanel {
  private state?:Status;
  private visible=false;
  private checking=false;
  private polling=false;
  private starting=false;
  private error='';
  private message='';
  private experimental=false;
  private releases=new Map<string,Release[]>();
  private selection=new Map<string,string>();
  private errors=new Map<string,string>();
  private file:File|null=null;
  private uploadTarget='application';
  private releaseUrl='';
  private reconnecting=false;
  private packageInput=el('input');
  constructor(private root:HTMLElement,private hooks:Hooks){}
  inProgress(){return this.starting||!!this.state?.job&&(!this.state.job.finished||this.state.job.phase==='recovery_required');}
  isReconnecting(){return this.reconnecting&&this.inProgress();}
  open(){this.visible=true;this.render();void this.poll();}
  close(){this.visible=false;}
  async poll(){
    if(this.polling||!this.visible&&!this.inProgress())return;
    this.polling=true;
    try{
      const state=await this.hooks.api<Status>('/api/updates');
      const changed=JSON.stringify(state)!==JSON.stringify(this.state)||this.reconnecting;
      this.state=state;this.reconnecting=false;
      if(changed){this.render();this.hooks.changed();}
    }catch{
      this.reconnecting=true;
      if(this.state?.status_port&&this.inProgress()){
        try{
          const url=new URL('/api/update-status',location.href);url.port=String(this.state.status_port);
          const response=await fetch(url,{cache:'no-store',signal:AbortSignal.timeout(3000)});
          if(response.ok){const status=await response.json() as {job:Job|null};if(status.job)this.state.job=status.job;}
        }catch{/* The independent supervisor may also be reconnecting after a reboot. */}
      }
      this.render();this.hooks.changed();
    }finally{this.polling=false;}
  }
  private helpUrl(){const url=new URL('/help/updates',location.href);if(this.reconnecting&&this.state?.status_port)url.port=String(this.state.status_port);return url.href;}
  private notify(){this.render();this.hooks.changed();}
  private async check(target='all',releaseUrl?:string){
    if(this.checking||this.inProgress())return;
    this.checking=true;this.error='';this.message='Checking GitHub releases…';this.notify();
    try{
      const result=await this.hooks.api<{targets?:Record<string,Release[]>;errors?:Record<string,string>;releases?:Release[]}>('/api/updates/check',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({target,experimental:this.experimental,release_url:releaseUrl||undefined})});
      if(result.targets){for(const [id,list] of Object.entries(result.targets)){this.releases.set(id,list);this.selection.set(id,list[0]?.tag??'');this.errors.delete(id);}}
      if(result.errors)for(const [id,error] of Object.entries(result.errors)){this.errors.set(id,error);this.releases.delete(id);}
      if(result.releases){this.releases.set(target,result.releases);this.selection.set(target,result.releases[0]?.tag??'');this.errors.delete(target);}
      this.message='Release check complete. Review a release before updating.';
    }catch(error){this.error=error instanceof Error?error.message:String(error);this.message='';}
    finally{this.checking=false;this.notify();}
  }
  private async begin(target:Target,release?:Release){
    if(this.inProgress())return;
    if(this.hooks.unsaved()){this.error='Save or discard your layout changes before updating. Your draft is kept.';this.notify();return;}
    const caption=release?`${release.tag} for ${target.name}`:`the selected package for ${target.name}`;
    if(!await this.hooks.confirm(`Update ${target.name}?`,`Paperphase will verify ${caption}, show Updating on the screen, then restart. Keep power connected. Your layout and data are kept. Save changes in any other open tabs first.`,'Update and restart'))return;
    this.starting=true;this.error='';this.message='Preparing your update…';this.notify();
    try{
      const options:RequestInit=release?{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({target:target.id,tag:release.tag,experimental:this.experimental})}:{method:'POST',headers:{'Content-Type':'application/octet-stream'},body:this.file!};
      const path=release?'/api/updates/install':`/api/updates/upload?target=${encodeURIComponent(target.id)}`;
      const job=await this.hooks.api<Job>(path,options);if(this.state)this.state.job=job;
      this.notify();requestAnimationFrame(()=>{const progress=this.root.querySelector<HTMLElement>('#update-progress');progress?.focus({preventScroll:true});progress?.scrollIntoView({block:'start'});});
      this.message='';
    }catch(error){this.error=error instanceof Error?error.message:String(error);this.message='';}
    finally{this.starting=false;this.notify();void this.poll();}
  }
  render(){this.hooks.retain(()=>this.build());}
  private build(){
    this.root.replaceChildren();
    const header=el('header','','updates-heading');const title=el('div');title.append(el('h2','Software updates'),el('p','Update Paperphase and its plugins without using SSH.','subtle'));header.append(title);this.root.append(header);
    if(!this.state){this.root.append(el('p',this.reconnecting?'Cannot reach Paperphase. Reconnect to your device, then try again.':'Loading installed software…','subtle'));if(this.reconnecting)this.root.append(button('Try again',()=>void this.poll()));return;}
    if(!this.state.managed){
      const setup=el('section','','update-setup');setup.append(el('h3','Update setup needed'),el('p','This development installation uses manual updates. Set up the update supervisor once to enable updates here. Your existing software keeps working.'));
      const advanced=el('details');advanced.append(el('summary','Installation setup'));advanced.append(el('p','The supervisor verifies signed release packages, preserves your saved data and recovers if a new version fails to start.'));
      const link=el('a','Read installation instructions');link.href=this.helpUrl();link.target='_blank';link.rel='noopener noreferrer';advanced.append(link);setup.append(advanced);this.root.append(setup);return;
    }
    const blocked=this.inProgress();
    const check=button(this.checking?'Checking releases…':'Check for updates',()=>void this.check(),this.checking||blocked);check.className='primary';header.append(check);
    if(this.hooks.unsaved())this.root.append(el('p','Save or discard your layout changes before installing an update. You can still check releases.','update-advisory'));
    if(this.error){const error=el('p',this.error,'notice error');error.setAttribute('role','alert');this.root.append(error);}
    if(this.message){const message=el('p',this.message,'subtle');message.setAttribute('role','status');this.root.append(message);}
    const job=this.state.job;
    if(job){
      const progress=el('section','','update-progress');progress.id='update-progress';progress.tabIndex=-1;progress.setAttribute('aria-label','Update progress');
      progress.append(el('h3',job.phase==='healthy'?'Update complete':job.phase==='rolled_back'?'Previous version restored':job.phase==='failed'?'Update was not installed':job.phase==='recovery_required'?'Recovery needed':'Update in progress'));
      const text=el('p',job.message);text.setAttribute('role',job.finished&&job.phase!=='healthy'?'alert':'status');progress.append(text);
      if(!job.finished){
        const steps=['Download','Verify','Updating screen','Restart & check'];const phases=[['downloading'],['verifying'],['waiting_for_display','ready_for_activation'],['backing_up','switching','starting','rolling_back']];
        const index=phases.findIndex(group=>group.includes(job.phase));const list=el('ol','','update-steps');
        steps.forEach((step,i)=>{const item=el('li',step,i===index?'current':i<index?'complete':'');if(i===index)item.setAttribute('aria-current','step');list.append(item);});progress.append(list);
      }
      if(this.reconnecting)progress.append(el('p','The main app is reconnecting. This page follows the supervisor while Paperphase restarts.','subtle'));
      if(job.phase==='recovery_required')progress.append(el('p','Your previous version and backup are retained. Use the recovery instructions under Advanced update options.','subtle'));
      this.root.append(progress);
    }
    const targets=this.state.targets;
    for(const [kind,items] of [['Paperphase',targets.filter(t=>t.id==='application')],['Plugins',targets.filter(t=>t.id!=='application')]] as const){
      if(!items.length)continue;
      const section=el('section','','update-software');section.append(el('h3',kind));this.root.append(section);
      for(const target of items){
        const row=el('div','','update-row');row.dataset.updateTarget=target.id;
        const identity=el('div','','update-identity');identity.append(el('strong',target.name),el('small',target.installed?`${target.installed.version} · ${target.installed.build}`:target.id==='application'?`${this.state.version} · Local installation`:'Installed locally'));row.append(identity);
        const options=el('div','','update-release');row.append(options);
        const releases=this.releases.get(target.id);const error=this.errors.get(target.id);
        if(error){options.append(el('p',error,'field-error'));options.append(button('Try again',()=>void this.check(target.id),this.checking||blocked));}
        else if(releases?.length){
          const selected=releases.find(r=>r.tag===this.selection.get(target.id))??releases[0]!;
          const label=el('label');const labelText=el('span',`Release for ${target.name}`);labelText.id=`update-release-label-${target.id}`;label.append(labelText);const select=el('select');select.id=`update-release-${target.id}`;select.setAttribute('aria-labelledby',labelText.id);select.disabled=blocked;
          for(const release of releases){const option=el('option',`${release.tag}${release.prerelease?' · Experimental':''}${release.tag===target.installed?.tag?' · Installed':''}`);option.value=release.tag;select.append(option);}select.value=selected.tag;select.onchange=()=>{this.selection.set(target.id,select.value);this.render();};label.append(select);options.append(label);
          const details=el('details','','release-notes');details.id=`release-notes-${target.id}`;const summary=el('summary','Release notes');summary.id=`update-notes-${target.id}`;details.append(summary);details.append(el('p',selected.notes||'No release notes supplied.','release-notes-text'));
          const link=el('a','View release on GitHub');link.href=selected.url;link.target='_blank';link.rel='noopener noreferrer';details.append(link);options.append(details);
          const installed=selected.tag===target.installed?.tag;
          const update=button(installed?'Installed release':'Update',()=>void this.begin(target,selected),blocked||installed||!target.publisher_configured||this.hooks.unsaved());update.setAttribute('aria-label',`Update ${target.name}`);if(!installed)update.className='primary';options.append(update);
        }else options.append(el('p',releases?'No compatible releases are published for this target.':'Check for updates to see available releases.','subtle'));
        if(!target.publisher_configured)options.append(el('p','Publisher setup needed before installing.','subtle'));
        section.append(row);
      }
    }
    const advanced=el('details','','update-advanced');advanced.id='update-advanced';advanced.append(el('summary','Advanced update options'));
    const experimental=el('input');experimental.id='update-experimental';experimental.type='checkbox';experimental.checked=this.experimental;experimental.disabled=blocked||this.checking;
    const experimentalLabel=el('label','','check');experimentalLabel.append(experimental,document.createTextNode('Include experimental pre-releases'));advanced.append(experimentalLabel);
    experimental.onchange=()=>{this.experimental=experimental.checked;this.releases.clear();this.selection.clear();this.message='Check again to use the selected release channel.';this.render();};
    const targetLabel=el('label');targetLabel.append(el('span','Software to update'));const target=el('select');target.id='update-upload-target';target.disabled=blocked;
    for(const item of targets){const option=el('option',item.name);option.value=item.id;target.append(option);}target.value=this.uploadTarget;target.onchange=()=>{this.uploadTarget=target.value;};targetLabel.append(target);advanced.append(targetLabel);
    const urlLabel=el('label');urlLabel.append(el('span','GitHub release URL'));const url=el('input');url.id='update-release-url';url.required=true;url.type='url';url.value=this.releaseUrl;url.placeholder='https://github.com/owner/repository/releases/tag/v0.1.0';url.disabled=blocked;url.oninput=()=>{this.releaseUrl=url.value;};urlLabel.append(url);advanced.append(urlLabel);
    advanced.append(button('Check this release',()=>{if(url.reportValidity())void this.check(this.uploadTarget,this.releaseUrl);},blocked||this.checking));
    const fileLabel=el('label');fileLabel.append(el('span','Signed release package (.ppkg)'));const file=this.packageInput;file.id='update-package-file';file.type='file';file.accept='.ppkg';file.disabled=blocked;file.onchange=()=>{this.file=file.files?.[0]??null;this.render();};fileLabel.append(file);advanced.append(fileLabel);
    if(this.file)advanced.append(el('p',`Selected: ${this.file.name}`,'subtle'));
    advanced.append(button('Upload and update',()=>{const selected=targets.find(t=>t.id===this.uploadTarget);if(selected)void this.begin(selected);},blocked||!this.file||this.hooks.unsaved()));
    const recovery=el('a','Update setup and SSH recovery instructions');recovery.href=this.helpUrl();recovery.target='_blank';recovery.rel='noopener noreferrer';advanced.append(recovery);this.root.append(advanced);
  }
}
