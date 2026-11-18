"""ECC start from ROW-EVEN renders only. Predict pad (term4 + zeros thru
next CW boundary + EC11...) from data_end; first deviation = ECC start."""
import sys
sys.path.insert(0, "/tmp/opencode")
from rsunit2 import stream_law, cws_of
from funcsolve import expected_stream

def pad_pattern(data_end):
    """infinite pad bitstream from data_end: 0000, zeros thru next CW
    boundary (grid-6), then EC11 alternating."""
    bits = []
    pos = data_end
    bits += [0,0,0,0]; pos += 4
    # zeros through end of next CW boundary
    nxt = ((pos - 6 + 7)//8)*8 + 6      # first grid-6 boundary > pos-1
    nxt2 = nxt + 8                       # end of the NEXT codeword
    bits += [0]*(nxt2 - pos); pos = nxt2
    # EC11 forever
    while len(bits) < 200:
        bits += [1,1,1,0,1,1,0,0]
        bits += [0,0,0,1,0,0,0,1]
    return bits

res = {}
PAY = ['a','aa','aaa','aaaa','aaaaa','aaaaaa','aaaaaaa','aaaaaaaa','aaaaaaaaa',
       '!!!!','!!!','!!!!!','1234567890','ABCDEFGH','[[]][][]','~~[[<>']
for lv in (1,2,3):
    devs = []
    for s in PAY:
        name, t = stream_law(s, lv)
        if name != 'row-even': continue
        E = expected_stream(s)
        de = len(E)
        pp = pad_pattern(de)
        d = None
        for i in range(de, 190):
            if t[i] != pp[i-de]: d = i; break
        devs.append((len(s), d))
    print(f"lv{lv}: (n, first_dev) = {devs}")
