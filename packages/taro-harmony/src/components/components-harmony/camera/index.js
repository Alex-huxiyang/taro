import { createOption } from '../utils'

export default createOption({
  props: [
    'id',
    'cls',
    'flash',
    'devicePosition'
  ],
  onError () {
    this.$trigger('error', { errMsg: '用户不允许使用摄像头' })
  }
})
