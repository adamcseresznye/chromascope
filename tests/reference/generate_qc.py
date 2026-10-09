"""Independent QC reference: Python statistics + exact rational OLS and ratios.
Only synthetic input observations are generated; production never reads expected outputs.
Run python tests/reference/generate_qc.py; stdlib only.
"""
import json
from pathlib import Path
from fractions import Fraction
from statistics import mean, stdev
rows=[]
def add(group, values, responses=None, role='validation', nominal=10):
    for i,value in enumerate(values):
        rows.append(dict(id=f'{group}-{i}',target='a',batch='run-1',group=group,role=role,order=len(rows)+1,unit='ng/mL',response_unit='instrument intensity * minute',value=value,nominal=nominal,response=responses[i] if responses else value,is_response=None,rt_minutes=None,expected_rt_minutes=None,mass_error_ppm=None,calibration_accepted=None,source=f'synthetic-study-record:{group}-{i}'))
add('qc',[9,10,11],role='qc')
for row, response, rt, ppm in zip(rows,[80,100,120],[1,1.01,1.02],[-2,0,4]):
    row.update(is_response=response,rt_minutes=rt,expected_rt_minutes=1,mass_error_ppm=ppm)
add('high',[100],role='standard',nominal=100)
add('blank',[.5],role='blank',nominal=0)
add('pre',[10,10],[60,80])
add('post',[10,10],[100,100])
add('neat',[10,10],[200,200])
add('stable',[10,10],[90,100])
add('fresh',[10,10],[100,100])
add('reference',[8,10,12])
add('calibration',[10,10,10],role='standard')
for row, accepted in zip(rows[-3:],[True,False,True]): row['calibration_accepted']=accepted
add('missing',[10,None],role='qc')
add('fit-standards',[1,2,3,4],[3,5,7,9],role='standard')
for row, nominal in zip(rows[-4:],[1,2,3,4]): row['nominal']=nominal
rules=[]
expected={}
def rule(metric,group,value,lower,upper,reference=None,n=1):
    rid=metric+'-'+group
    rules.append(dict(id=rid,target='a',group=group,role=None,batch=None,metric=metric,lower=lower,upper=upper,minimum_n=n,required=True,reference_group=reference))
    expected[rid]=float(value)
rule('accuracy','qc',100,95,105,n=3)
rule('maximum_accuracy_deviation','qc',10,0,10,n=3)
rule('precision','qc',100*stdev([9,10,11])/mean([9,10,11]),0,10,n=3)
rule('internal_standard_cv','qc',100*stdev([80,100,120])/mean([80,100,120]),0,20,n=3)
rule('rt_drift','qc',.02,0,.03,n=3)
rule('mass_error','qc',4,0,5,n=3)
x=[Fraction(1),Fraction(2),Fraction(3)]; y=list(map(Fraction,[9,10,11]))
slope=sum((a-mean(x))*(b-mean(y)) for a,b in zip(x,y))/sum((a-mean(x))**2 for a in x)
rule('order_slope','qc',slope,-2,2,n=3)
rule('rt_drift_slope','qc',Fraction(1,100),0,.02,n=3)
rule('mass_drift_slope','qc',3,0,4,n=3)
rule('internal_standard_slope','qc',20,0,25,n=3)
rule('blank','blank',Fraction(1,2),0,1)
rule('carryover','blank',Fraction(1,2),0,1)
rule('recovery','pre',70,60,80,'post',2)
rule('matrix_factor','post',50,40,60,'neat',2)
rule('process_efficiency','pre',35,30,40,'neat',2)
rule('selectivity','blank',Fraction(1,4),0,1,'neat')
rule('stability','stable',95,90,110,'fresh',2)
rule('control_chart','qc',max(abs(v-mean([8,10,12]))/stdev([8,10,12]) for v in [9,10,11]),0,3,'reference',3)
rule('calibration_acceptance','calibration',Fraction(200,3),60,100,n=3)
rule('missingness','missing',50,0,50,n=2)
rule('dilution_integrity','qc',100,95,105,n=3)
calibration=dict(degree=1,intercept=dict(kind='free'),weighting='unweighted',unit='ng_ml',range=[1,4],lod=0,loq=1,accuracy_tolerance_percent=15,qc_cv_limit_percent=15,blank_response_limit=0)
for metric,value,limits in [('calibration_r2',1,[.99,1]),('calibration_rmse',0,[0,1e-8]),('calibration_accuracy_deviation',0,[0,1e-8])]:
    rule(metric,'fit-standards',value,*limits,n=4)
    rules[-1]['calibration']=calibration
for row in rows:
    row['preparation']={'pre':'pre_extraction','post':'post_extraction','neat':'neat','blank':'blank','stable':'stored','fresh':'fresh'}.get(row['group'])
study=dict(version=1,method='synthetic-reference-assay',purpose='method_validation',observations=rows,rules=rules,targeted_evidence=None)
Path(__file__).with_name('qc.json').write_text(json.dumps(dict(study=study,expected=expected),indent=2)+'\n')
request=dict(version=1,operation_id='be63f4c2-c3d7-4f32-b877-a5974e04aa08',actor='example-user',operation=dict(operation='validate_method',study=study))
Path('examples/qc-request.json').write_text(json.dumps(request,indent=2)+'\n')


