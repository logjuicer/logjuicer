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
import {
  Alert,
  Button,
  ListView,
} from 'patternfly-react'


import {
  updateAnomaly,
  removeLogfile,
} from '../../api'
import BuildLine from './BuildLine'
import LogFile from './LogFile'
import CopyAnomalyModal from './CopyAnomalyModal'



class Anomaly extends React.Component {
  static propTypes = {
    anomaly: PropTypes.object.isRequired,
    history: PropTypes.object.isRequired
  }

  state = {
    filesToRemove: {},
    copyForm: false,
  }

  constructor (props) {
    super(props)
    this.modalRef = React.createRef()
  }

  submitRemove = async () => {
    const removeList = Object.assign(this.state.filesToRemove)
    /*function sleeper(ms) {
      return function(x) {
        return new Promise(resolve => setTimeout(() => resolve(x), ms));
      };
    }*/
    for (var logfileId in removeList) {
      const logfile = removeList[logfileId]
      await removeLogfile(this.props.anomaly.uuid, logfileId)
      // eslint-disable-next-line
        .then(() => {
          // Remove logfile from props list
          this.props.anomaly.logfiles.splice(
            this.props.anomaly.logfiles.indexOf(logfile),
            1
          )
          // Remove logfile from toRemove list
          const newList = Object.assign(this.state.filesToRemove)
          delete newList[logfileId]
          this.props.anomaly.status = 'reviewed'
          this.setState({filesToRemove: newList})
        })
      .catch(error => {
        throw (error)
      })
    }
  }

  submitArchived = () => {
    updateAnomaly(this.props.anomaly.uuid, {status: 'archive'})
      .then(() => {
        //this.props.anomaly.status = 'archived'
        this.forceUpdate()
      })
      .catch(error => {
        throw (error)
      })
  }

  submitReviewed = () => {
    updateAnomaly(this.props.anomaly.uuid, {status: 'reviewed'})
      .then(() => {
        this.props.anomaly.status = 'reviewed'
        this.forceUpdate()
        window.scrollTo(0, 0)
      })
      .catch(error => {
        throw (error)
      })
  }

  render () {
    const { anomaly } = this.props
    const removeList = []
    for (var logfileId in this.state.filesToRemove) {
      removeList.push(this.state.filesToRemove[logfileId])
    }
    let extraRows = ''
    if (['processed', 'reviewed', 'archived'].indexOf(anomaly.status) !== -1) {
      extraRows = (
        <React.Fragment>
          {anomaly.status === 'archived' && (
            <tr><td>Archive date</td><td>{anomaly.archive_date}</td></tr>)}
          <tr><td>Build</td><td><BuildLine build={anomaly.build} /></td></tr>
          <tr><td>Baselines</td><td>{anomaly.baselines.map(build => (
            <BuildLine key={build.uuid} build={build} />
          ))}</td></tr>
          <tr><td>Train command</td><td>{anomaly.train_command}</td></tr>
          <tr><td>Test command</td><td>{anomaly.test_command}</td></tr>
        </React.Fragment>
      )
    }
    const info = (
      <table className='table table-condensed table-responsive table-bordered'>
        <tbody>
          <tr><td>Name</td><td>{anomaly.name}</td></tr>
          <tr><td>Status</td><td>{anomaly.status} {
                anomaly.status === 'archived' && (
                  <a href={'/log-classify/datasets/' +
                     anomaly.uuid.slice(0, 2) + '/' + anomaly.uuid}
                     rel='noopener noreferrer'
                     target='_blank'>
                    dataset link
                  </a>
              )}</td></tr>
          <tr><td>Reporter</td><td>{anomaly.reporter}</td></tr>
          <tr><td>Report date</td><td>{anomaly.report_date}</td></tr>
          {extraRows}
        </tbody>
      </table>
    )
    return (
      <div>
        <CopyAnomalyModal
          anomaly={this}
          ref={this.modalRef}
          />
        <div className='pull-left'>
          <Button
            onClick={() => {this.modalRef.current.setState({show: true})}}
            bsStyle='primary'
            title='Create a copy of the report'>
            Duplicate
          </Button>&nbsp;
        </div>
        {anomaly.status === 'reviewed' && (
          <div className='pull-left'>
            <Button onClick={this.submitArchived} bsStyle='success'>
              Archive
            </Button>&nbsp;
          </div>)}

        {anomaly.status === 'reviewed' && (
          <p>This report has been reviewed, click the <i>
              Archive</i> button to generate a dataset.</p>
        )}
        {info}
        {anomaly.status === 'processed' && (
          <p>This report is under review. Please remove irrelevant files,
            false positive lines  and/or adjust the scores.
            Once done, click the submit button.</p>
        )}
        <ListView>
          {anomaly.logfiles
            .filter(item => !this.state.filesToRemove.hasOwnProperty(item.id))
            .map((logfile, idx) => (
              <LogFile key={idx} logfile={logfile} idx={idx} anomaly={this} />
        ))}
        </ListView>
        {removeList.length > 0 && (
          <Alert onDismiss={() => this.setState({filesToRemove: {}})}>
            <p>File to be removed:</p>
            <ul>
              {removeList.map(item => (
                <li key={item.id}>Path: {item.path}</li>)
               )}
            </ul>
            <br />
            <Button onClick={this.submitRemove} bsStyle='danger'>
              Submit removal
            </Button>
          </Alert>
        )}
        {removeList.length === 0 && anomaly.status === 'processed' && (
          <Button onClick={this.submitReviewed} bsStyle='success'>
            I have reviewed this report and it looks correct.
          </Button>
        )}
      </div>
    )
  }
}

export default Anomaly
