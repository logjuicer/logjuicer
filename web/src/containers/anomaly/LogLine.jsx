// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import React from 'react'
import PropTypes from 'prop-types'
import update from 'immutability-helper'
import {
  Icon
} from 'patternfly-react'


class LogLine extends React.Component {
  static propTypes = {
    info: PropTypes.array.isRequired,
    line: PropTypes.string.isRequired,
    logfile: PropTypes.object.isRequired,
    removed: PropTypes.bool.isRequired,
    idx: PropTypes.number.isRequired,
    newblock: PropTypes.bool.isRequired
  }

  state = {
    background: '#FFFFFF'
  }

  leftPad (n, width) {
    n = n + ''
    return n.length >= width ? n : new Array(
      width - n.length + 1).join('0') + n
  }

  componentDidMount () {
    this.setBackground()
  }

  setBackground = () => {
    var r = Math.floor(255 - 142 * this.props.info[1]).toString(16)
    if (r.length === 1) {
      r = 'F' + r
    }
    this.setState({background: '#FF' + r + r})
  }

  updateConfidence = (evt) => {
    this.props.info[1] = evt.target.value / 100
    this.setBackground()
    if (!this.props.logfile.state.dirtyScores) {
      this.props.logfile.setState({dirtyScores: true})
    }
  }

  toggle = (idxList) => {
    const { scoresIndexToRemove } = this.props.logfile.state
    let dirty = true
    let action
    if (this.props.removed) {
      if (scoresIndexToRemove.length === idxList.length) {
        dirty = false
      }
      let idxPos = idxList.map(item => scoresIndexToRemove.indexOf(item))
      idxPos.sort(function (a,b) {return a - b})
      idxPos.reverse()
      action = {$splice: idxPos.map(item => [item, 1])}
    } else {
      action = {$push: idxList}
    }
    this.props.logfile.setState({
      dirty: dirty,
      scoresIndexToRemove: update(scoresIndexToRemove, action)
    })
  }

  toggleGroup = () => {
    const scores = this.props.logfile.props.logfile.scores
    const toToggle = [this.props.idx]
    for (let idx = this.props.idx; idx < scores.length - 1; idx += 1) {
      if (scores[idx][0] + 1 !== scores[idx + 1][0]) {
        break
      }
      // Contigous line
      toToggle.push(idx + 1)
    }
    this.toggle(toToggle)
    this.props.logfile.forceUpdate()
  }

  render () {
    const { line, info, removed, newblock } = this.props
    if (!line) {
      return (<p>Loading...</p>)
    }
    const style = {
      background: this.state.background
    }
    const lineDom = (
      <span className='LogLine' style={style}>
        {this.leftPad(info[0], 3)} | {line}
        <br />
      </span>
    )
    if (this.props.logfile.props.anomaly.props.anomaly.status === 'archived') {
      return lineDom
    }
    if (removed) {
      style.textDecoration = 'line-through'
    }
    return (
      <React.Fragment>
        <input
          type='range'
          min='0'
          max='100'
          value={info[1] * 100}
          className='LogSlider'
          onChange={this.updateConfidence}
          />
        {lineDom}
        {newblock ? (
          <span className='LogControl'>
            <Icon
              type='pf'
              name={removed ? 'enhancement' : 'close'}
              onClick={this.toggleGroup}
              title='Toggle group'
              />
            <Icon
              type='pf'
              name={removed ? 'enhancement' : 'delete'}
              onClick={() => {this.toggle([this.props.idx])}}
              title='Toggle line'
              />
          </span>
          ) : (
         <Icon
          type='pf'
          name={removed ? 'enhancement' : 'delete'}
          className='LogControl'
          onClick={() => {this.toggle([this.props.idx])}}
          title='Toggle line'
          />

        )}
      </React.Fragment>
    )
  }
}

export default LogLine
