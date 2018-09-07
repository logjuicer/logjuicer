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
  Button,
  DropdownKebab,
  MenuItem,
  Icon,
  ListView,
  Modal
} from 'patternfly-react'
import * as moment from 'moment'


import {
  updateLogfile
} from '../../api'
import BuildLine from './BuildLine'
import CopyAnomalyModal from './CopyAnomalyModal'
import LogLine from './LogLine'


class LogFile extends React.Component {
  static propTypes = {
    anomaly: PropTypes.object.isRequired,
    logfile: PropTypes.object.isRequired,
    idx: PropTypes.number.isRequired
  }

  state = {
    dirty: false,
    dirtyScores: false,
    scoresIndexToRemove: [],
    showModel: false,
  }

  constructor (props) {
    super(props)
    this.fileListView = React.createRef()
    this.modalRef = React.createRef()
  }

  submit = () => {
    const { anomaly, logfile } = this.props
    const { scoresIndexToRemove } = this.state
    scoresIndexToRemove.sort(function (a,b) {return a - b})
    const indexToRemove = []
    for (let pos = scoresIndexToRemove.length - 1; pos >= 0; pos -= 1) {
      indexToRemove.push([scoresIndexToRemove[pos], 1])
    }
    logfile.scores = update(logfile.scores, {$splice: indexToRemove})
    logfile.lines = update(logfile.lines, {$splice: indexToRemove})
    this.setState({
      scoresIndexToRemove: []
    })
    updateLogfile(anomaly.props.anomaly.uuid, logfile.id, logfile.scores)
      .then(() => {
        this.setState({
          dirty: false,
          dirtyScores: false,
        })
        anomaly.props.anomaly.status = 'reviewed'
        anomaly.forceUpdate()
      })
      .catch(error => {
        throw (error)
      })
  }

  remove = () => {
    const filesToRemove = {}
    filesToRemove[this.props.logfile.id] = this.props.logfile
    this.props.anomaly.setState({
      filesToRemove: Object.assign(
        filesToRemove, this.props.anomaly.state.filesToRemove)
    })
  }

  componentDidMount () {
    if (this.props.idx === 0) {
      this.fileListView.current.setState({expanded: true})
    }
  }

  render () {
    const { scoresIndexToRemove } = this.state
    const { anomaly, logfile } = this.props
    const AdditionalInfo = [(
      <ListView.InfoItem key={1}>
        <Icon type='fa' name='bug' />
        <strong>{logfile.scores.length}</strong>
      </ListView.InfoItem>
    ), (
      <ListView.InfoItem key={2}>
        <Icon type='pf' name='registry' />
        <a
          onClick={() => {this.setState({showModel: true})}}
          style={{
            cursor: 'pointer',
            zIndex: 10,
            pointerEvents: 'auto',
            borderBottomStyle: 'none',
            textDecoration: 'none'
          }} >
        <strong>{logfile.model}</strong>
        </a>
      </ListView.InfoItem>
    )]
    if (logfile.test_time > 60) {
      AdditionalInfo.push((
      <ListView.InfoItem key={2}>
        <Icon type='fa' name='clock-o' />
        <strong>
          tested {moment.duration(logfile.test_time, 'seconds').humanize(true)}
        </strong>
      </ListView.InfoItem>
      ))
    }
    const model = anomaly.props.anomaly.models[logfile.model]
    const modelInfo = (
      <Modal
          show={this.state.showModel}
          onHide={() => {this.setState({showModel: false})}}
          dialogClassName='ModelModal'
        >
        <Modal.Header>
          <button
            className='close'
            onClick={() => {this.setState({showModel: false})}}
            aria-hidden='true'
            aria-label='Close'
            >
            <Icon type='pf' name='close' />
          </button>
          <Modal.Title>Model information of {logfile.model}</Modal.Title>
        </Modal.Header>
        <Modal.Body>
          <table
            className='table table-condensed table-responsive table-bordered'>
            <tbody>
              <tr><td>Name</td><td>{logfile.model}</td></tr>
              <tr><td>Info</td><td>{model.info}</td></tr>
              <tr><td>Train time</td><td>{model.train_time} seconds</td></tr>
            </tbody>
          </table>
          <h3>Source files</h3>
          <ul className='list-group'>
            {model.source_files.map((item, idx) => (
              <li className='list-group-item' key={idx}>{item}</li>
            ))}
          </ul>
          <h3>Source Builds</h3>
          <ul className='list-group'>
            {model.source_builds.map((item, idx) => (
              <li className='list-group-item' key={idx}>
                <BuildLine build={anomaly.props.anomaly.baselines.filter(
                             baseline => baseline['uuid'] === item)[0]} />
              </li>
            ))}
          </ul>
        </Modal.Body>
      </Modal>
    )
    const Heading = (
      <div>{logfile.path} (
        <a href={anomaly.props.anomaly.build.log_url + logfile.path}>
          log link</a>
      )</div>
    )
    return (
      <React.Fragment>
        {modelInfo}
        <CopyAnomalyModal
          anomaly={anomaly}
          logfile={logfile}
          ref={this.modalRef}
          />
      <ListView.Item
        heading={Heading}
        additionalInfo={AdditionalInfo}
        actions={(
          <div>
            {anomaly.props.anomaly.logfiles.length > 1 &&
             anomaly.props.anomaly.status !== 'archived' && (
               <Button bsStyle='danger' onClick={this.remove}>Remove</Button>
             )}
            <DropdownKebab id='logfileAction' pullRight>
            <MenuItem
              onClick={() => {this.modalRef.current.setState({show: true})}}>
                Copy to new anomaly
              </MenuItem>
            </DropdownKebab>
          </div>)}
        active='true'
        ref={this.fileListView}
        hideCloseIcon={true}
        expanded
        >
        {logfile.lines
          .map((line, idx) => (
            <React.Fragment key={logfile.scores[idx][0]}>
            {(idx > 0 &&
              logfile.scores[idx - 1][0] + 1 !== logfile.scores[idx][0]) &&
              <hr />}
            <div key={logfile.scores[idx][0]} style={{display: 'flex'}}>
              <LogLine
                idx={idx}
                line={line}
                info={logfile.scores[idx]}
                logfile={this}
                newblock={(idx === 0 ||
                logfile.scores[idx - 1][0] + 1 !== logfile.scores[idx][0])}
                removed={scoresIndexToRemove.indexOf(idx) !== -1}/>
          </div>
</React.Fragment>
        ))}
      {(this.state.dirty || this.state.dirtyScores) &&
       (anomaly.props.anomaly.status !== 'archived') &&
        <Button
          bsStyle='primary'
          onClick={this.submit}>
          Update{scoresIndexToRemove.length > 0 &&
                   ' (delete ' + scoresIndexToRemove.length + ' line(s)'}
        </Button>
      }
      </ListView.Item>
        </React.Fragment>
    )
  }
}

export default LogFile
