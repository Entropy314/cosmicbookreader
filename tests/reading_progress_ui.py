"""Exercise the compiled UI with a fake Tauri bridge; no Drive account is used.

Build with trunk build --dist /tmp/reader-dist, then run:
python3 tests/reading_progress_ui.py --dist /tmp/reader-dist --browser /path/to/chrome-headless-shell
"""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import subprocess
import tempfile
import threading

BRIDGE = r'''<script>
const empty = () => ({status:'unread',last_page:null,updated_at:0});
let tick = 100;
const books = [
 {id:'drive-saga-1',title:'Saga Chapter 1',series:'Saga',downloaded:true,page_count:3,reading:{status:'completed',last_page:2,updated_at:1}},
 {id:'drive-saga-2',title:'Saga Chapter 2',series:'Saga',downloaded:true,page_count:3,reading:{status:'reading',last_page:0,updated_at:2}},
 {id:'drive-saga-3',title:'Saga Chapter 3',series:'Saga',downloaded:false,page_count:null,reading:empty()},
 {id:'local-other',title:'Another Title 1',series:'Another Title',downloaded:true,page_count:3,reading:empty()}
].map(b=>({...b,path:'/mock/'+b.id+'.cbz',format:'cbz'}));
window.calls=[];window.errors=[];
let failPage=false, slowPage=false, failSave=false, slowSave=false;
const pause = (ms=100) => new Promise(resolve=>setTimeout(resolve,ms));
const page = index => ({index,data_uri:'data:image/svg+xml,'+encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="600" height="800"><rect width="600" height="800" fill="#e5dfd4"/><text x="240" y="400" font-size="30">Page ${index+1}</text></svg>`)});
window.addEventListener('error',e=>errors.push(e.message));
window.addEventListener('unhandledrejection',e=>errors.push(String(e.reason)));
window.__TAURI__={core:{invoke:async(cmd,args)=>{
 calls.push({cmd,args});
 if(['get_library','refresh_library'].includes(cmd)) return structuredClone(books);
 if(cmd==='get_cover') return null;
 if(cmd==='get_drive_status') return {configured:true,connected:true,folder_id:'mock',folder_name:'Comics',auto_sync:true,sync_mode:'index_only',last_sync:null,busy:false,message:'',error:null,completed:4,total:4,revision:0,downloading_comic_id:null};
 const book=books.find(b=>b.id===args?.comicId);
 if(cmd==='open_comic') {
  book.downloaded=true;book.page_count=3;
  return {comic:structuredClone(book),page_count:3,page:page(book.reading.last_page??0)};
 }
 if(cmd==='get_page') {
  if(args.pageIndex===2 && failPage) throw 'Page unavailable';
  await pause(args.pageIndex===2 && slowPage ? 300 : 15);
  return page(args.pageIndex);
 }
 if(cmd==='save_progress') {
  if(failSave) throw 'Disk unavailable';
  await pause(slowSave ? 180 : 10);
  book.reading={status:book.reading.status==='completed'||args.page===2?'completed':'reading',last_page:args.page,updated_at:++tick};
  return structuredClone(book.reading);
 }
 if(cmd==='set_reading_status') {
  for(const id of args.comicIds){
   const b=books.find(b=>b.id===id);
   b.reading=args.status==='unread'?empty():{...b.reading,status:'completed',updated_at:++tick};
  }
  return null;
 }
 if(cmd==='close_comic')return null;
 throw 'Unexpected command '+cmd;
}}};
window.addEventListener('TrunkApplicationStarted',async()=>{
 const check=(ok,message)=>{if(!ok)throw Error(message)};
 const button=label=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===label);
 const action=label=>[...document.querySelectorAll('.series-reading-actions a')].find(a=>a.textContent.trim()===label);
 const row=id=>[...document.querySelectorAll('.book-row')].find(r=>r.querySelector('.book-name').getAttribute('href')==='/read/'+id);
 const jump=n=>{const el=document.querySelector('[aria-label="Go to page"]');el.value=String(n);el.dispatchEvent(new Event('change',{bubbles:true}));};
 const status=(id,value)=>check(books.find(b=>b.id===id).reading.status===value,id+' status should be '+value);
 const back=async()=>{button('← Library').click();await pause();};
 const mark=async(id,label)=>{row(id).querySelector('.book-more').click();await pause();button(label).click();await pause();};
 try {
  await pause();
  check(document.querySelector('.title-grid').textContent.includes('1 of 3 completed'),'Title progress missing');
  [...document.querySelectorAll('.series-open')].find(b=>b.textContent.includes('Saga')).click();await pause();
  check(action('Continue reading →').getAttribute('href')==='/read/drive-saga-2','Continue chose the wrong chapter');
  check(action('Read next unread →').getAttribute('href')==='/read/drive-saga-3','Next unread did not skip completed/reading chapters');
  check(!calls.some(c=>c.cmd==='open_comic'),'Browsing opened a book');
  action('Continue reading →').click();await pause(180);
  check(!calls.some(c=>c.cmd==='save_progress'&&c.args.page===1),'Prefetch advanced progress');
  failPage=true;jump(3);await pause();
  check(document.querySelector('.reader-error'),'Page failure missing');
  check(!calls.some(c=>c.cmd==='save_progress'&&c.args.page===2),'Failed page was recorded');
  status('drive-saga-2','reading');
  document.querySelector('.reader-error a').click();await pause();failPage=false;
  action('Continue reading →').click();await pause(150);
  slowPage=true;jump(3);jump(1);await pause(420);
  check(document.querySelector('[aria-label="Go to page"]').value==='1','Stale page replaced the latest request');
  check(!calls.some(c=>c.cmd==='save_progress'&&c.args.page===2),'Stale final page marked completed');
  slowPage=false;slowSave=true;jump(3);await pause(40);jump(2);await pause(480);slowSave=false;
  status('drive-saga-2','completed');
  check(books[1].reading.last_page===1,'Serialized saves lost the latest position');
  await back();
  check(document.querySelector('.series-progress-summary').textContent.includes('2 of 3 completed'),'Completion count did not update');
  check(!action('Continue reading →'),'Completed book still offered as unfinished');
  check(action('Read next unread →').getAttribute('href')==='/read/drive-saga-3','Next unread selected wrong series/book');
  await mark('drive-saga-3','Mark completed');
  check(document.querySelector('.all-read'),'All-completed state missing');
  check(!action('Read next unread →'),'All-completed title should not cross into another title');
  check(!calls.some(c=>c.cmd==='open_comic'&&c.args.comicId==='drive-saga-3'),'Manual status downloaded a book');
  await mark('drive-saga-2','Mark unread · reset progress');
  check(books[1].reading.last_page===null,'Mark unread did not reset position');
  check(action('Read next unread →').getAttribute('href')==='/read/drive-saga-2','Reset book not selected next');
  await mark('drive-saga-2','Mark completed');
  await mark('drive-saga-3','Mark unread · reset progress');
  button('Available offline').click();await pause();
  check(!action('Read next unread →'),'Offline filter offered a cloud-only chapter');
  button('Everything').click();await pause();
  failSave=true;action('Read next unread →').click();await pause(160);
  check(document.querySelector('.progress-save-error'),'Save failure was hidden');
  check(!document.querySelector('.reader-error'),'Save failure blocked reading');
  failSave=false;button('Retry saving').click();await pause();
  check(!document.querySelector('.progress-save-error'),'Retry did not clear save error');
  status('drive-saga-3','reading');await back();
  check(row('drive-saga-3').textContent.includes('Reading · page 1 of 3'),'Chapter page progress missing');
  check(document.documentElement.scrollWidth<=innerWidth,'Horizontal overflow');
  check(errors.length===0,errors.join(' | '));
  document.documentElement.dataset.smokeResult='passed';
 }catch(error){document.documentElement.dataset.smokeResult='failed: '+error.message;}
});
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dist', type=Path, required=True)
    parser.add_argument('--browser', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=Path('/tmp/cosmic-reading-progress-check'))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    document = (args.dist / 'index.html').read_text().replace('<head>', '<head>' + BRIDGE).encode()

    class Handler(SimpleHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path == '/' or self.path.startswith('/read/'):
                self.send_response(200)
                self.send_header('Content-Type', 'text/html; charset=utf-8')
                self.end_headers()
                self.wfile.write(document)
            else:
                super().do_GET()

    server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Handler, directory=str(args.dist.resolve())))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for width in (1280, 480):
            with tempfile.TemporaryDirectory(prefix='cosmic-progress-browser-') as profile:
                result = subprocess.run([
                    str(args.browser.resolve()), '--no-sandbox', '--disable-gpu', '--hide-scrollbars',
                    '--user-data-dir=' + profile, f'--window-size={width},960', '--virtual-time-budget=9000',
                    '--screenshot=' + str((args.output / f'{width}.png').resolve()), '--dump-dom',
                    f'http://127.0.0.1:{server.server_port}/',
                ], capture_output=True, text=True, timeout=40)
                (args.output / f'{width}.html').write_text(result.stdout)
                passed = 'data-smoke-result="passed"' in result.stdout
                print(f'{width}px: {"PASS" if passed else "FAIL"}', flush=True)
                if not passed:
                    print(result.stdout[:1200])
                    print(result.stderr[-2500:])
                    raise SystemExit(1)
    finally:
        server.shutdown()


if __name__ == '__main__':
    main()
