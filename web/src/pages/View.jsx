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
import { withRouter } from 'react-router-dom'
import { connect } from 'react-redux'
import {
  Button,
  Icon,
  Modal,
} from 'patternfly-react'

import { fetchAnomalyAction } from '../reducers'
import { deleteAnomaly } from '../api'
import Anomaly from '../containers/anomaly/Anomaly'


class AnomalyView extends React.Component {
  static propTypes = {
    match: PropTypes.object.isRequired,
    anomaly: PropTypes.object.isRequired,
    history: PropTypes.object.isRequired,
    dispatch: PropTypes.func
  }

  state = {
    showDelete: false
  }

  getAnomaly = () => {
    const { anomaly } = this.props
    return anomaly[this.props.match.params.anomalyId]
  }

  componentDidMount () {
    if (!this.getAnomaly()) {
      this.refresh()
    }
  }

  refresh = () => {
    console.log('Refreshing anomaly ' + this.props.match.params.anomalyId)
    this.props.dispatch(
      fetchAnomalyAction(this.props.match.params.anomalyId))
  }

  submitDelete = () => {
    deleteAnomaly(this.props.match.params.anomalyId)
      .then(() => {
        this.props.history.push('/list')
      })
      .catch(error => {
        throw (error)
      })
  }

  render () {
    const anomaly = this.getAnomaly()
    return (
      <React.Fragment>
        {anomaly && (
        <div className='pull-right'>
          <Button
            onClick={() => {this.setState({showDelete: true})}}
            bsStyle='danger'>
            Delete
          </Button>
        </div>)}
        <Modal show={this.state.showDelete}
               onHide={() => {this.setState({showDelete: false})}}>
          <Modal.Header>
            <button
              className='close'
              onClick={() => {this.setState({showDelete: false})}}
              aria-hidden='true'
              aria-label='Close'
              >
              <Icon type='pf' name='close' />
            </button>
            <Modal.Title>
              Delete this report
            </Modal.Title>
          </Modal.Header>
          <Modal.Body>
            <Button bsStyle='danger' onClick={this.submitDelete}>
              Delete
            </Button>
            &nbsp;&nbsp;
            <Button onClick={() => {this.setState({showDelete: false})}}>
              Cancel
            </Button>
          </Modal.Body>
        </Modal>
        <a className='refresh pull-right' onClick={this.refresh}>
          <Icon type='fa' name='refresh' /> refresh&nbsp;&nbsp;
        </a>
        {anomaly ? <Anomaly
                       anomaly={anomaly}
                       history={this.props.history} /> : <p>Loading...</p>}
      </React.Fragment>
    )
  }
}

export default withRouter(connect(
  state => ({
    anomaly: state.anomaly
  })
)(AnomalyView))
