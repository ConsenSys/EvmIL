.code
        push 0x00
        calldataload
        push 0x01        
        eq
        push lab0
        jumpi
        push 0x00
        push 0x00
        revert
lab0:
        jumpdest
