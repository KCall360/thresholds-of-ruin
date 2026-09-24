import json,pathlib
from reportlab.graphics.shapes import Drawing,Rect,String,Line
from reportlab.graphics.charts.lineplots import LinePlot
from reportlab.graphics.charts.axes import LogYValueAxis
from reportlab.graphics.widgets.markers import makeMarker
from reportlab.graphics import renderPDF
from reportlab.lib.colors import HexColor,white
import pypdfium2 as pdfium
out=pathlib.Path(__file__).resolve().parent
cases={r['case']:r for r in json.loads((out/'matrix-summary.json').read_text())}
d=Drawing(1000,650); d.add(Rect(0,0,1000,650,fillColor=white,strokeColor=None))
d.add(String(40,618,'Phase A: p95 authoritative command latency',fontName='Helvetica-Bold',fontSize=20))
d.add(String(40,595,'Identical local traces, five cycles; Windows / NTFS HDD; logarithmic y axes',fontSize=11,fillColor=HexColor('#485565')))
colors={8:'#247a9a',64:'#bb6334',256:'#7056a4'}
for k,(r,color) in enumerate(colors.items()):
 x=40+k*160; d.add(Line(x,570,x+22,570,strokeColor=HexColor(color),strokeWidth=2));d.add(String(x+28,566,f'{r} regions',fontSize=10))
d.add(Line(565,570,590,570,strokeColor=HexColor('#777777'),strokeWidth=1,strokeDashArray=[4,3]));d.add(String(598,566,'Provisional durable target: 8 ms',fontSize=10))
for i,actors in enumerate((1,8)):
 for j,mode in enumerate(('memory','durable')):
  x=70+j*490; y=345-i*270
  p=LinePlot();p.x=x;p.y=y;p.width=385;p.height=165
  p.data=[[(idx,cases[f'r{r}-a{actors}-h{h}-{mode}']['mixed']['authoritative_total']['p95_ms']) for idx,h in enumerate((0,100,1000,10000))] for r in colors]
  if mode=='durable':p.data.append([(0,8),(3,8)])
  p.xValueAxis.valueMin=0;p.xValueAxis.valueMax=3;p.xValueAxis.valueSteps=[0,1,2,3];p.xValueAxis.labelTextFormat=lambda v:['0','100','1,000','10,000'][int(v)]
  p.yValueAxis=LogYValueAxis();p.yValueAxis.valueMin=.5 if mode=='memory' else 5;p.yValueAxis.valueMax=30 if mode=='memory' else 2000;p.yValueAxis.valueSteps=[1,2,5,10,20] if mode=='memory' else [8,30,100,300,1000]
  p.yValueAxis.labelTextFormat=lambda v:f'{v:g}'
  for axis in (p.xValueAxis,p.yValueAxis):axis.labels.fontSize=9;axis.strokeColor=HexColor('#8b95a1');axis.strokeWidth=.5
  p.yValueAxis.visibleGrid=True;p.yValueAxis.gridStrokeColor=HexColor('#e0e5e8');p.yValueAxis.gridStrokeWidth=.5
  for k,color in enumerate(colors.values()):
   p.lines[k].strokeColor=HexColor(color);p.lines[k].strokeWidth=1.8;s=makeMarker('FilledCircle');s.size=5;s.fillColor=HexColor(color);s.strokeColor=HexColor(color);p.lines[k].symbol=s
  if mode=='durable':p.lines[3].strokeColor=HexColor('#777777');p.lines[3].strokeWidth=1;p.lines[3].strokeDashArray=[4,3]
  d.add(p);d.add(String(x,y+190,f'{mode.capitalize()} / {actors} actor'+('s' if actors>1 else ''),fontName='Helvetica-Bold',fontSize=13));d.add(String(x-40,y+169,'ms',fontSize=9));d.add(String(x+100,y-40,'Starting retained actions',fontSize=10))
doc=pdfium.PdfDocument(renderPDF.drawToString(d));doc[0].render(scale=1.5).to_pil().save(out/'latency-history.png')
