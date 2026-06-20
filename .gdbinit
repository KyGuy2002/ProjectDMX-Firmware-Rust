define localhost
    target remote localhost:3333
    monitor arm semihosting enable
    monitor arm semihosting_fileio enable
    set remote system-call-allowed 1
end

define reset
    monitor reset init
end
