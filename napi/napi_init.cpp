#include "napi/native_api.h"
#include "m3_ffi.h"
#include <new>
#include <vector>

namespace {
using Call=char*(*)(const char*);
struct Work{napi_async_work work{};napi_deferred deferred{};Call call{};std::vector<char> input;char* output{};};
void Execute(napi_env,void* data){auto*w=static_cast<Work*>(data);w->output=w->call(w->input.data());}
void Complete(napi_env env,napi_status status,void* data){auto*w=static_cast<Work*>(data);if(status!=napi_ok||!w->output){napi_value m=nullptr,e=nullptr;napi_create_string_utf8(env,"M3 task runtime failed",NAPI_AUTO_LENGTH,&m);napi_create_error(env,nullptr,m,&e);napi_reject_deferred(env,w->deferred,e);}else{napi_value v=nullptr;napi_create_string_utf8(env,w->output,NAPI_AUTO_LENGTH,&v);napi_resolve_deferred(env,w->deferred,v);}if(w->output)m3_free_string(w->output);if(w->work)napi_delete_async_work(env,w->work);delete w;}
napi_value Call(napi_env env,napi_callback_info info,Call fn,const char* name){size_t argc=1,n=0;napi_value arg,res,label;napi_valuetype type;if(napi_get_cb_info(env,info,&argc,&arg,nullptr,nullptr)!=napi_ok||argc!=1||napi_typeof(env,arg,&type)!=napi_ok||type!=napi_string){napi_throw_type_error(env,nullptr,"expected one JSON string");return nullptr;}if(napi_get_value_string_utf8(env,arg,nullptr,0,&n)!=napi_ok||n>16*1024*1024){napi_throw_range_error(env,nullptr,"request exceeds 16 MiB");return nullptr;}auto*w=new(std::nothrow)Work{};if(!w){napi_throw_error(env,nullptr,"out of memory");return nullptr;}w->input.resize(n+1);size_t copied=0;if(napi_get_value_string_utf8(env,arg,w->input.data(),w->input.size(),&copied)!=napi_ok||copied!=n){delete w;napi_throw_type_error(env,nullptr,"invalid UTF-8 input");return nullptr;}w->call=fn;if(napi_create_string_utf8(env,name,NAPI_AUTO_LENGTH,&label)!=napi_ok||napi_create_promise(env,&w->deferred,&res)!=napi_ok||napi_create_async_work(env,nullptr,label,Execute,Complete,w,&w->work)!=napi_ok||napi_queue_async_work(env,w->work)!=napi_ok){delete w;napi_throw_error(env,nullptr,"failed to schedule task runtime work");return nullptr;}return res;}
napi_value Propose(napi_env e,napi_callback_info i){return Call(e,i,m3_propose_plan_json,"M3 propose plan");}napi_value Validate(napi_env e,napi_callback_info i){return Call(e,i,m3_validate_plan_json,"M3 validate plan");}napi_value Start(napi_env e,napi_callback_info i){return Call(e,i,m3_start_json,"M3 start task");}napi_value Decide(napi_env e,napi_callback_info i){return Call(e,i,m3_decide_backend_json,"M3 decide backend");}
napi_value Exports(napi_env e,napi_value o){napi_property_descriptor p[]={{"proposePlan",nullptr,Propose,nullptr,nullptr,nullptr,napi_default,nullptr},{"validatePlan",nullptr,Validate,nullptr,nullptr,nullptr,napi_default,nullptr},{"start",nullptr,Start,nullptr,nullptr,nullptr,napi_default,nullptr},{"decideBackend",nullptr,Decide,nullptr,nullptr,nullptr,napi_default,nullptr}};return napi_define_properties(e,o,4,p)==napi_ok?o:nullptr;}
napi_module module={1,0,nullptr,Exports,"xiaohong_agent_runtime",nullptr,{0}};
}
extern "C" __attribute__((constructor)) void RegisterXiaohongRuntime(){napi_module_register(&module);}
